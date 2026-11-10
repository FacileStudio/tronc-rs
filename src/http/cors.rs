use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, HeaderValue, Method, StatusCode};

/// The methods a cross-origin caller may use when [`CorsConfig::allowed_methods`] is empty.
pub const DEFAULT_ALLOWED_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
/// The request headers allowed when [`CorsConfig::allowed_headers`] is empty.
pub const DEFAULT_ALLOWED_HEADERS: [&str; 4] = ["Accept", "Authorization", "Content-Type", "X-Request-Id"];
/// The response headers exposed when [`CorsConfig::exposed_headers`] is `None`.
pub const DEFAULT_EXPOSED_HEADERS: [&str; 1] = ["X-Request-Id"];

/// Which cross-origin callers an app accepts, as Go `middleware.CORSConfig`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CorsConfig {
    /// Origins matched exactly. `*` allows every origin, only without credentials. Empty
    /// denies every cross-origin request and mounts no CORS layer.
    pub allowed_origins: Vec<String>,
    /// Sends `Access-Control-Allow-Credentials: true`.
    pub allow_credentials: bool,
    /// Defaults to [`DEFAULT_ALLOWED_HEADERS`].
    pub allowed_headers: Vec<String>,
    /// Defaults to [`DEFAULT_ALLOWED_METHODS`].
    pub allowed_methods: Vec<String>,
    /// Defaults to [`DEFAULT_EXPOSED_HEADERS`]; `Some(vec![])` exposes none.
    pub exposed_headers: Option<Vec<String>>,
    /// Defaults to 600.
    pub max_age_seconds: u32,
}

pub struct Cors {
    origins: Vec<String>,
    wildcard: bool,
    decorations: Vec<(&'static str, HeaderValue)>,
}

fn joined(values: &[String], fallback: &[&str]) -> String {
    if values.is_empty() { fallback.join(", ") } else { values.join(", ") }
}

impl Cors {
    pub fn new(config: &CorsConfig) -> Result<Self, String> {
        let wildcard = config.allowed_origins.iter().any(|origin| origin == "*");
        if wildcard && config.allow_credentials {
            return Err("tronc/middleware: CORS allows origin \"*\" with AllowCredentials, which lets any site read authenticated responses; list the origins explicitly or turn credentials off".into());
        }
        let exposed =
            config.exposed_headers.as_ref().map_or_else(|| DEFAULT_EXPOSED_HEADERS.join(", "), |h| h.join(", "));
        let max_age = if config.max_age_seconds == 0 { 600 } else { config.max_age_seconds };
        let mut pairs = vec![
            ("access-control-allow-methods", joined(&config.allowed_methods, &DEFAULT_ALLOWED_METHODS)),
            ("access-control-allow-headers", joined(&config.allowed_headers, &DEFAULT_ALLOWED_HEADERS)),
            ("access-control-max-age", max_age.to_string()),
        ];
        if !exposed.is_empty() {
            pairs.push(("access-control-expose-headers", exposed));
        }
        if config.allow_credentials {
            pairs.push(("access-control-allow-credentials", "true".into()));
        }
        let decorations = pairs
            .into_iter()
            .map(|(name, value)| HeaderValue::from_str(&value).map(|v| (name, v)))
            .collect::<Result<_, _>>()
            .map_err(|_| "tronc/middleware: CORS header value is not valid".to_owned())?;
        Ok(Self { origins: config.allowed_origins.clone(), wildcard, decorations })
    }

    fn decorate(&self, headers: &mut HeaderMap, origin: &HeaderValue) {
        for vary in ["Origin", "Access-Control-Request-Method", "Access-Control-Request-Headers"] {
            headers.append(http::header::VARY, HeaderValue::from_static(vary));
        }
        let allow = if self.wildcard { HeaderValue::from_static("*") } else { origin.clone() };
        headers.insert(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, allow);
        for (name, value) in &self.decorations {
            headers.insert(*name, value.clone());
        }
    }
}

pub async fn cors(State(cors): State<Arc<Cors>>, request: Request, next: Next) -> Response {
    let Some(origin) = request.headers().get(http::header::ORIGIN).filter(|o| !o.is_empty()).cloned() else {
        return next.run(request).await;
    };
    let preflight = request.method() == Method::OPTIONS;
    let allowed = cors.wildcard || cors.origins.iter().any(|o| o.as_bytes() == origin.as_bytes());
    let mut response = match (allowed, preflight) {
        (false, true) => return StatusCode::FORBIDDEN.into_response(),
        (false, false) => return next.run(request).await,
        (true, true) => StatusCode::NO_CONTENT.into_response(),
        (true, false) => next.run(request).await,
    };
    cors.decorate(response.headers_mut(), &origin);
    response
}
