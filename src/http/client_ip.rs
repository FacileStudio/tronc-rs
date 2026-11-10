use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use http::HeaderMap;
use http::request::Parts;
use ipnet::IpNet;

use crate::proxies::{parse_addr, trusted_by};

/// Which forwarded headers to believe, as Go `middleware.RealIPConfig`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RealIpConfig {
    /// The peers whose `X-Forwarded-For` is read at all. Empty trusts nothing.
    pub trusted: Vec<IpNet>,
    /// The CDN edge ranges whose `header` is believed. Empty disables the CDN case.
    pub cdn: Vec<IpNet>,
    /// The CDN's visitor header, as in `Cf-Connecting-Ip`.
    pub header: String,
}

/// The caller's address after client-IP resolution: the visitor's bare IP when a trusted
/// proxy named one, otherwise the connection's `ip:port`, as Go's rewritten `RemoteAddr`. It is
/// what a rate limiter keys on. Empty when the server was not started with connect info.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteAddr(pub String);

impl<S: Send + Sync> FromRequestParts<S> for RemoteAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<RemoteAddr>().cloned().unwrap_or_else(|| RemoteAddr(peer_string(&parts.extensions))))
    }
}

/// The connection's address as `ip:port`, or empty without connect info.
pub fn peer_string(extensions: &http::Extensions) -> String {
    extensions.get::<ConnectInfo<SocketAddr>>().map(|info| info.0.to_string()).unwrap_or_default()
}

/// Resolves the address a request should be attributed to, as Go `RealIPWith`: the rightmost
/// `X-Forwarded-For` hop that is not a trusted proxy, believed only from a trusted peer, and
/// the CDN header only when the rightmost hop is a CDN edge.
pub fn resolve(remote_addr: &str, headers: &HeaderMap, config: &RealIpConfig) -> String {
    match forwarded_client(remote_addr, headers, &config.trusted).or_else(|| cdn_client(remote_addr, headers, config)) {
        Some(ip) => ip.to_string(),
        None => remote_addr.to_owned(),
    }
}

fn trusted_peer(remote_addr: &str, trusted: &[IpNet]) -> bool {
    parse_addr(remote_addr).is_some_and(|peer| trusted_by(peer, trusted))
}

fn forwarded(headers: &HeaderMap) -> Option<&str> {
    headers.get("x-forwarded-for").and_then(|value| value.to_str().ok()).filter(|value| !value.is_empty())
}

fn forwarded_client(remote_addr: &str, headers: &HeaderMap, trusted: &[IpNet]) -> Option<IpAddr> {
    if !trusted_peer(remote_addr, trusted) {
        return None;
    }
    for hop in forwarded(headers)?.rsplit(',') {
        let hop = parse_addr(hop)?;
        if !trusted_by(hop, trusted) {
            return Some(hop);
        }
    }
    None
}

fn cdn_client(remote_addr: &str, headers: &HeaderMap, config: &RealIpConfig) -> Option<IpAddr> {
    if config.cdn.is_empty() || config.header.is_empty() || !trusted_peer(remote_addr, &config.trusted) {
        return None;
    }
    let edge = parse_addr(forwarded(headers)?.rsplit(',').next()?)?;
    if !trusted_by(edge, &config.cdn) {
        return None;
    }
    parse_addr(headers.get(config.header.as_str())?.to_str().ok()?)
}

/// The caller's address for logging only: `Cf-Connecting-Ip`, then the first
/// `X-Forwarded-For` hop, then the remote address. Never key a limit on it.
pub fn log_client_ip(headers: &HeaderMap, remote_addr: &str) -> String {
    let header = |name: &str| headers.get(name).and_then(|value| value.to_str().ok()).filter(|value| !value.is_empty());
    if let Some(cloudflare) = header("cf-connecting-ip") {
        return cloudflare.to_owned();
    }
    if let Some(forwarded) = header("x-forwarded-for") {
        return forwarded.split(',').next().unwrap_or("").trim().to_owned();
    }
    remote_addr.to_owned()
}
