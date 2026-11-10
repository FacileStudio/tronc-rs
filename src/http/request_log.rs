use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::header::USER_AGENT;
use tracing::Level;

use super::client_ip::{RemoteAddr, log_client_ip, peer_string};
use super::query::redact_query;
use super::request_id::RequestId;

/// What a request is serving, so API traffic stands apart from health probes and assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Under the API prefix.
    Api,
    /// `/health` or `/ready`, at the root or under the API prefix.
    Health,
    /// Anything else: the client's files.
    Static,
}

impl Kind {
    /// The logged spelling: `api`, `health` or `static`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Health => "health",
            Self::Static => "static",
        }
    }
}

/// Classifies a path, as Go `middleware.Classify`. An empty prefix means the API is at the root.
pub fn classify(path: &str, api_prefix: &str) -> Kind {
    let health = ["/health", "/ready"];
    if health.contains(&path) || health.iter().any(|probe| path.strip_prefix(api_prefix) == Some(probe)) {
        return Kind::Health;
    }
    if path == api_prefix || path.strip_prefix(api_prefix).is_some_and(|rest| rest.starts_with('/')) {
        return Kind::Api;
    }
    Kind::Static
}

/// The level a request is logged at: info for the API, debug for the rest, error on any 5xx.
pub fn level_for(kind: Kind, status: u16) -> Level {
    match (status >= 500, kind) {
        (true, _) => Level::ERROR,
        (false, Kind::Api) => Level::INFO,
        (false, _) => Level::DEBUG,
    }
}

pub struct LogConfig {
    pub api_prefix: String,
}

macro_rules! record {
    ($level:expr, $($field:tt)*) => {
        match $level {
            Level::ERROR => tracing::error!($($field)*),
            Level::WARN => tracing::warn!($($field)*),
            Level::INFO => tracing::info!($($field)*),
            Level::DEBUG => tracing::debug!($($field)*),
            Level::TRACE => tracing::trace!($($field)*),
        }
    };
}

pub async fn log_requests(State(config): State<Arc<LogConfig>>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let path = request.uri().path().to_owned();
    let query = redact_query(request.uri().query().unwrap_or(""));
    let method = request.method().to_string();
    let request_id = request.extensions().get::<RequestId>().map(|id| id.0.clone()).unwrap_or_default();
    let remote =
        request.extensions().get::<RemoteAddr>().map_or_else(|| peer_string(request.extensions()), |r| r.0.clone());
    let client_ip = log_client_ip(request.headers(), &remote);
    let user_agent = request.headers().get(USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let kind = classify(&path, &config.api_prefix);
    let bytes = axum::body::HttpBody::size_hint(response.body()).exact().unwrap_or(0);
    let duration = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    record!(
        level_for(kind, status),
        kind = kind.as_str(),
        request_id,
        method,
        path,
        query,
        remote_addr = remote,
        client_ip,
        user_agent,
        status,
        bytes,
        duration,
        "http request"
    );
    response
}
