use std::time::Duration;

use tronc::env::{Core, Environment, Vars};

#[test]
fn core_defaults_match_go() {
    let core = Core::from_vars(&Vars::from_pairs([("DATABASE_URL", "postgres://x")]), true).expect("core");
    assert_eq!(core.port, 8080);
    assert_eq!(core.log_level, "info");
    assert_eq!(core.app_env, Environment::Development);
    assert_eq!(core.trusted_proxies, tronc::proxies::default_trusted());
    assert!(core.cors_allowed_origins.is_empty() && core.cdn_header.is_empty());
}

#[test]
fn database_url_is_required_unless_waived() {
    let empty = Vars::from_pairs::<&str, &str>([]);
    assert_eq!(Core::from_vars(&empty, true).unwrap_err().to_string(), "env: DATABASE_URL is required");
    assert!(Core::from_vars(&empty, false).is_ok());
}

#[test]
fn malformed_values_name_the_variable() {
    let vars = Vars::from_pairs([("PORT", "eighty"), ("FLAG", "yes"), ("WAIT", "5 minutes")]);
    assert_eq!(Core::from_vars(&vars, false).unwrap_err().to_string(), "env: PORT must be an integer, got \"eighty\"");
    assert!(vars.bool("FLAG", false).is_err());
    assert!(vars.duration("WAIT", Duration::ZERO).is_err());
}

#[test]
fn cors_origins_fall_back_through_the_drifted_names() {
    let vars = Vars::from_pairs([
        ("DOMAIN", " https://a.example , ,https://b.example"),
        ("CLIENT_ORIGIN", "https://c.example"),
    ]);
    assert_eq!(tronc::env::cors_origins(&vars), ["https://a.example", "https://b.example"]);
}

#[test]
fn go_durations_and_booleans_parse() {
    let vars = Vars::from_pairs([("A", "1h30m"), ("B", "300ms"), ("C", "1.5s"), ("D", "T"), ("E", "0")]);
    assert_eq!(vars.duration("A", Duration::ZERO), Ok(Duration::from_secs(5400)));
    assert_eq!(vars.duration("B", Duration::ZERO), Ok(Duration::from_millis(300)));
    assert_eq!(vars.duration("C", Duration::ZERO), Ok(Duration::from_millis(1500)));
    assert_eq!(vars.bool("D", false), Ok(true));
    assert_eq!(vars.duration("E", Duration::from_secs(9)), Ok(Duration::ZERO));
    assert_eq!(vars.duration("UNSET", Duration::from_secs(9)), Ok(Duration::from_secs(9)));
}

#[test]
fn cloudflare_in_trusted_proxies_turns_on_the_cdn_header() {
    let vars = Vars::from_pairs([("TRUSTED_PROXIES", "private,cloudflare"), ("APP_ENV", "prod")]);
    let core = Core::from_vars(&vars, false).expect("core");
    assert_eq!(core.cdn_header, "Cf-Connecting-Ip");
    assert!(core.is_production());
    assert_eq!(core.trusted_proxies.len(), tronc::proxies::PRIVATE.len() + tronc::proxies::CLOUDFLARE.len());
}

#[test]
fn trusted_proxies_none_trusts_nothing() {
    let core = Core::from_vars(&Vars::from_pairs([("TRUSTED_PROXIES", "NONE")]), false).expect("core");
    assert!(core.trusted_proxies.is_empty());
}
