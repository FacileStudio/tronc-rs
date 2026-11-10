//! Proxy trust sets: which peers may speak for the visitor in a forwarded header.

use std::net::IpAddr;

use ipnet::IpNet;

/// Loopback and the private ranges: the peer every Facile app sees behind Traefik.
pub const PRIVATE: [&str; 8] = [
    "127.0.0.0/8",
    "::1/128",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "fc00::/7",
    "fe80::/10",
];

/// Cloudflare's published edge ranges, the same snapshot Go tronc carries (fetched 2026-08-10).
pub const CLOUDFLARE: [&str; 22] = [
    "173.245.48.0/20",
    "103.21.244.0/22",
    "103.22.200.0/22",
    "103.31.4.0/22",
    "141.101.64.0/18",
    "108.162.192.0/18",
    "190.93.240.0/20",
    "188.114.96.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
    "162.158.0.0/15",
    "104.16.0.0/13",
    "104.24.0.0/14",
    "172.64.0.0/13",
    "131.0.72.0/22",
    "2400:cb00::/32",
    "2606:4700::/32",
    "2803:f800::/32",
    "2405:b500::/32",
    "2405:8100::/32",
    "2a06:98c0::/29",
    "2c0f:f248::/32",
];

/// The default trust set, [`PRIVATE`] parsed.
pub fn default_trusted() -> Vec<IpNet> {
    parse(PRIVATE).unwrap_or_default()
}

/// [`CLOUDFLARE`] parsed.
pub fn cloudflare() -> Vec<IpNet> {
    parse(CLOUDFLARE).unwrap_or_default()
}

/// Parses CIDR blocks or bare addresses into prefixes, masking host bits, as Go
/// `middleware.ParseTrustedProxies` does. Blank entries are skipped.
pub fn parse<I, S>(values: I) -> Result<Vec<IpNet>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut prefixes = Vec::new();
    for value in values {
        let trimmed = value.as_ref().trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(net) = trimmed.parse::<IpNet>() {
            prefixes.push(net.trunc());
            continue;
        }
        let addr = trimmed.parse::<IpAddr>().map_err(|_| format!("invalid CIDR address or IP: {trimmed}"))?;
        prefixes.push(IpNet::from(unmap(addr)));
    }
    Ok(prefixes)
}

/// Reports whether `addr` falls in any of the prefixes.
pub fn trusted_by(addr: IpAddr, trusted: &[IpNet]) -> bool {
    let addr = unmap(addr);
    trusted.iter().any(|net| net.contains(&addr))
}

/// Reads `host:port`, a bare address or the bracketed IPv6 form, as Go tronc's `parseAddr`.
pub fn parse_addr(value: &str) -> Option<IpAddr> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(addr) = trimmed.parse::<IpAddr>() {
        return Some(unmap(addr));
    }
    split_host(trimmed).and_then(|host| host.parse::<IpAddr>().ok()).map(unmap)
}

/// Splits `host:port` or `[host]:port` and returns the host, as Go `net.SplitHostPort`.
pub fn split_host(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        return after.starts_with(':').then_some(host);
    }
    let (host, _) = value.rsplit_once(':')?;
    (!host.contains(':')).then_some(host)
}

fn unmap(addr: IpAddr) -> IpAddr {
    match addr {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(addr, IpAddr::V4),
        IpAddr::V4(_) => addr,
    }
}
