use std::net::IpAddr;

use http::{HeaderMap, HeaderName, HeaderValue};
use ipnet::IpNet;

use super::csp::{self, Sources};
use super::policy;
use crate::proxies;

/// The Strict-Transport-Security value sent on HTTPS requests.
pub const HSTS_VALUE: &str = "max-age=31536000; includeSubDomains";

/// The CSP length past which the chassis warns at startup: proxies cap response headers.
pub const MAX_CSP_HEADER_BYTES: usize = 4096;

/// Shapes the security headers. The default is the suite policy with the CSP in report-only
/// mode, as Go `middleware.SecurityConfig`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecurityConfig {
    /// Sends the full policy as `Content-Security-Policy`. Off, the full policy is report-only
    /// and only `frame-ancestors`, `object-src` and `base-uri` are enforced.
    pub csp_enforce: bool,
    /// Sources added to a directive, keyed by directive name.
    pub csp_sources: Sources,
    /// Appends a `report-uri` directive when set.
    pub csp_report_uri: String,
    /// Permissions-Policy features granted to `(self)`.
    pub allow: Vec<String>,
    /// Cross-Origin-Opener-Policy, default `same-origin`.
    pub cross_origin_opener_policy: String,
    /// Drops `includeSubDomains` from HSTS.
    pub hsts_exclude_subdomains: bool,
    /// The peers whose `X-Forwarded-Proto` is believed. `None` takes the router's trusted
    /// proxies; an empty list trusts none.
    pub trusted: Option<Vec<IpNet>>,
}

/// A compiled security policy, ready to stamp onto responses.
#[derive(Debug, Clone)]
pub struct Security {
    fixed: Vec<(HeaderName, HeaderValue)>,
    hsts: HeaderValue,
    trusted: Vec<IpNet>,
}

impl Security {
    /// Compiles `config`. A source, directive, feature or opener policy that would corrupt a
    /// header is refused with the message Go tronc panics with.
    pub fn new(config: &SecurityConfig, default_trusted: &[IpNet]) -> Result<Self, String> {
        let coop = policy::opener(&config.cross_origin_opener_policy)?;
        let directives = csp::build(&config.csp_sources, &config.csp_report_uri)?;
        let permissions = policy::permissions(&config.allow)?;
        let mut pairs = csp_headers(&directives, config);
        pairs.push(("x-content-type-options", "nosniff".into()));
        pairs.push(("referrer-policy", "strict-origin-when-cross-origin".into()));
        pairs.push(("permissions-policy", permissions));
        pairs.push(("cross-origin-opener-policy", coop));
        for (name, value) in &pairs {
            if name.starts_with("content-security-policy") && value.len() > MAX_CSP_HEADER_BYTES {
                tracing::warn!(header = name, bytes = value.len(), "CSP header exceeds the size proxies reliably pass");
            }
        }
        let fixed = pairs.into_iter().map(|(name, value)| header(name, &value)).collect::<Result<_, _>>()?;
        let hsts = if config.hsts_exclude_subdomains { "max-age=31536000" } else { HSTS_VALUE };
        let trusted = config.trusted.clone().unwrap_or_else(|| default_trusted.to_vec());
        Ok(Self { fixed, hsts: HeaderValue::from_static(hsts), trusted })
    }

    /// Sets each header the response does not already carry, and HSTS when the visitor is on
    /// HTTPS.
    pub fn apply(&self, headers: &mut HeaderMap, https: bool) {
        for (name, value) in &self.fixed {
            if !headers.contains_key(name) {
                headers.insert(name.clone(), value.clone());
            }
        }
        if https && !headers.contains_key(http::header::STRICT_TRANSPORT_SECURITY) {
            headers.insert(http::header::STRICT_TRANSPORT_SECURITY, self.hsts.clone());
        }
    }

    /// Reports whether the visitor reached this server over HTTPS: the connection is TLS, or a
    /// trusted peer says so in the rightmost `X-Forwarded-Proto` value.
    pub fn is_https(&self, tls: bool, peer: Option<IpAddr>, request: &HeaderMap) -> bool {
        if tls {
            return true;
        }
        if !peer.is_some_and(|peer| proxies::trusted_by(peer, &self.trusted)) {
            return false;
        }
        let Some(last) = request.get_all("x-forwarded-proto").iter().next_back() else { return false };
        let last = last.to_str().unwrap_or("");
        last.rsplit(',').next().is_some_and(|hop| hop.trim().eq_ignore_ascii_case("https"))
    }
}

fn csp_headers(directives: &[csp::Directive], config: &SecurityConfig) -> Vec<(&'static str, String)> {
    let mut headers = if config.csp_enforce {
        vec![("content-security-policy", csp::render(directives))]
    } else {
        vec![
            ("content-security-policy-report-only", csp::render(directives)),
            ("content-security-policy", csp::render(&csp::framing(directives))),
        ]
    };
    if !framing_opened(config.csp_sources.get("frame-ancestors")) {
        headers.push(("x-frame-options", "DENY".into()));
    }
    headers
}

/// Reports whether `frame-ancestors` sources let any origin frame the page.
pub fn framing_opened(sources: Option<&Vec<String>>) -> bool {
    sources.is_some_and(|sources| sources.iter().any(|source| source != "'none'"))
}

fn header(name: &'static str, value: &str) -> Result<(HeaderName, HeaderValue), String> {
    let value =
        HeaderValue::from_str(value).map_err(|_| format!("tronc/middleware: {name} is not a valid header value"))?;
    Ok((HeaderName::from_static(name), value))
}
