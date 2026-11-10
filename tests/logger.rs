use tracing::Level;
use tronc::logger::parse_level;

#[test]
fn levels_parse_like_go_and_default_to_info() {
    let cases = [
        ("debug", Level::DEBUG),
        (" WARNING ", Level::WARN),
        ("warn", Level::WARN),
        ("error", Level::ERROR),
        ("verbose", Level::INFO),
        ("", Level::INFO),
    ];
    for (raw, want) in cases {
        assert_eq!(parse_level(raw), want, "{raw:?}");
    }
}

#[test]
fn init_twice_keeps_the_first_subscriber() {
    let first = tronc::logger::init(tronc::logger::Config::default());
    let vars = tronc::env::Vars::from_pairs([("LOG_LEVEL", "debug")]);
    let core = tronc::env::Core::from_vars(&vars, false).expect("core");
    let second = tronc::logger::init(tronc::logger::Config::from_core(&core));
    tracing::info!("still logging");
    drop((first, second));
}
