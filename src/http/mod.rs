//! The standard middleware chain every API runs, a port of Go `httpx` and `middleware`.
//!
//! Order, outermost first: request id, security headers, panic recovery, client IP, CORS,
//! request log. Go runs recovery above the security headers; Go's headers are written before
//! the handler runs, so its recovered 500 still carries them. Rust stamps headers on the way
//! out, so recovery sits just inside them to keep that answer. Security headers sit outside
//! client-IP resolution because the HSTS decision trusts `X-Forwarded-Proto` only from the
//! real peer.

mod chain;
mod client_ip;
mod cors;
mod csp;
mod directives;
mod override_policy;
mod policy;
mod query;
mod rate_limit;
mod recover;
mod request_id;
mod request_log;
mod security;

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use ipnet::IpNet;
use tower::util::BoxCloneSyncService;
use tower::{Layer, Service, ServiceBuilder};

pub use client_ip::{RealIpConfig, RemoteAddr, log_client_ip, resolve as resolve_client};
pub use cors::{CorsConfig, DEFAULT_ALLOWED_HEADERS, DEFAULT_ALLOWED_METHODS, DEFAULT_EXPOSED_HEADERS};
pub use csp::{Sources, build_csp};
pub use override_policy::{SecurityOverride, SecurityOverrideConfig, security_override};
pub use query::{SENSITIVE_QUERY_KEYS, redact_query};
pub use rate_limit::{DEFAULT_RATE_LIMIT, KeyFn, RATE_LIMIT_EXCEEDED_BODY, RateLimitConfig, RateLimiter, rate_limit};
pub use request_id::{MAX_REQUEST_ID_LENGTH, REQUEST_ID_HEADER, RequestId};
pub use request_log::{Kind, classify, level_for};
pub use security::{HSTS_VALUE, MAX_CSP_HEADER_BYTES, Security, SecurityConfig};

use crate::env::Core;

/// The standard chain's configuration, as Go `httpx.Config`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// The security headers. Always on.
    pub security: SecurityConfig,
    /// CORS, mounted only when `allowed_origins` is non-empty.
    pub cors: CorsConfig,
    /// The peers whose `X-Forwarded-For` is believed. `None` means [`crate::proxies::PRIVATE`];
    /// an empty list trusts nothing.
    pub trusted_proxies: Option<Vec<IpNet>>,
    /// CDN edge ranges whose visitor header is believed.
    pub cdn_proxies: Vec<IpNet>,
    /// The CDN's visitor header.
    pub cdn_header: String,
    /// Where the API lives, for log classification. `None` means `/api`; `Some("")` is root.
    pub api_prefix: Option<String>,
}

impl Config {
    /// The chain the core configuration describes: its proxies, CDN and CORS origins.
    pub fn from_core(core: &Core) -> Self {
        Self {
            cors: CorsConfig { allowed_origins: core.cors_allowed_origins.clone(), ..CorsConfig::default() },
            trusted_proxies: Some(core.trusted_proxies.clone()),
            cdn_proxies: core.cdn_proxies.clone(),
            cdn_header: core.cdn_header.clone(),
            ..Self::default()
        }
    }
}

/// A request extension marking a connection this process terminated TLS on. Insert it when
/// serving HTTPS in-process; behind Traefik the chassis reads `X-Forwarded-Proto` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tls;

/// The standard chain as one `tower` layer, for `axum::Router::layer`.
#[derive(Clone)]
pub struct Layers {
    compiled: Arc<chain::Compiled>,
    cors: Option<Arc<cors::Cors>>,
    log: Arc<request_log::LogConfig>,
}

/// Builds the standard chain. A configuration Go tronc would panic on at startup, such as a
/// CSP source with a `;` or a credentialed wildcard CORS origin, is an `Err` with Go's message.
pub fn layers(config: Config) -> Result<Layers, String> {
    let trusted = config.trusted_proxies.unwrap_or_else(crate::proxies::default_trusted);
    let security = Security::new(&config.security, &trusted)?;
    let real_ip = RealIpConfig { trusted, cdn: config.cdn_proxies, header: config.cdn_header };
    let cors =
        if config.cors.allowed_origins.is_empty() { None } else { Some(Arc::new(cors::Cors::new(&config.cors)?)) };
    let api_prefix = config.api_prefix.unwrap_or_else(|| "/api".to_owned());
    Ok(Layers {
        compiled: Arc::new(chain::Compiled { security, real_ip }),
        cors,
        log: Arc::new(request_log::LogConfig { api_prefix }),
    })
}

impl<S> Layer<S> for Layers
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Service = BoxCloneSyncService<Request, Response, Infallible>;

    fn layer(&self, inner: S) -> Self::Service {
        let inner =
            BoxCloneSyncService::new(from_fn_with_state(self.log.clone(), request_log::log_requests).layer(inner));
        let inner = match &self.cors {
            Some(cors) => BoxCloneSyncService::new(from_fn_with_state(cors.clone(), cors::cors).layer(inner)),
            None => inner,
        };
        let service = ServiceBuilder::new()
            .layer(from_fn(request_id::attach))
            .layer(from_fn_with_state(self.compiled.clone(), chain::security_headers))
            .layer(from_fn(recover::recover))
            .layer(from_fn_with_state(self.compiled.clone(), chain::real_ip))
            .service(inner);
        BoxCloneSyncService::new(service)
    }
}

/// The 404 Go's router answers with: `404 page not found` as plain text. Use it as the
/// router's fallback so an unknown path reads the same from a Rust app.
pub async fn not_found() -> Response {
    plain(StatusCode::NOT_FOUND, "404 page not found\n")
}

/// A plain-text answer as Go `http.Error` writes it, with `nosniff`.
pub fn plain(status: StatusCode, body: &'static str) -> Response {
    let mut response = (status, body).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}
