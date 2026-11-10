use ipnet::IpNet;

use super::{EnvError, Vars};
use crate::proxies;

/// Reads `TRUSTED_PROXIES`: CIDR blocks, bare addresses and the names `private` and
/// `cloudflare`. Unset means [`proxies::default_trusted`]; `none` trusts nothing.
pub fn trusted(vars: &Vars) -> Result<Vec<IpNet>, EnvError> {
    let Some(raw) = vars.get("TRUSTED_PROXIES") else { return Ok(proxies::default_trusted()) };
    if raw.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    let mut prefixes = Vec::new();
    let mut literals = Vec::new();
    for entry in vars.list("TRUSTED_PROXIES") {
        match entry.to_ascii_lowercase().as_str() {
            "private" => prefixes.extend(proxies::default_trusted()),
            "cloudflare" => prefixes.extend(proxies::cloudflare()),
            _ => literals.push(entry),
        }
    }
    let parsed = proxies::parse(&literals).map_err(|reason| {
        EnvError(format!(
            "TRUSTED_PROXIES takes CIDR blocks, IP addresses, or the names private and cloudflare: {reason}"
        ))
    })?;
    prefixes.extend(parsed);
    Ok(prefixes)
}

/// The CDN edge ranges and visitor header implied by `TRUSTED_PROXIES`.
pub fn cdn(vars: &Vars) -> (Vec<IpNet>, String) {
    if vars.list("TRUSTED_PROXIES").iter().any(|entry| entry.eq_ignore_ascii_case("cloudflare")) {
        return (proxies::cloudflare(), "Cf-Connecting-Ip".to_owned());
    }
    (Vec::new(), String::new())
}
