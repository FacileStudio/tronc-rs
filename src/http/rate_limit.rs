use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};

use super::client_ip::{RemoteAddr, peer_string};

/// The refill rate a limiter gets when [`RateLimitConfig::limit`] is zero: 10 requests a second.
pub const DEFAULT_RATE_LIMIT: f64 = 10.0;
/// The plain-text body of a refusal.
pub const RATE_LIMIT_EXCEEDED_BODY: &str = "rate limit exceeded";

/// Derives the bucket a request spends from.
pub type KeyFn = Arc<dyn Fn(&Request) -> String + Send + Sync>;

/// A per-key token bucket, as Go `middleware.RatelimitConfig`.
#[derive(Clone, Default)]
pub struct RateLimitConfig {
    /// Sustained requests per second. Zero or less means [`DEFAULT_RATE_LIMIT`].
    pub limit: f64,
    /// Requests allowed at once. Zero means `limit` rounded up.
    pub burst: u32,
    /// The bucket identity. `None` keys on the host of [`RemoteAddr`], so mount it inside
    /// [`super::layers`].
    pub key: Option<KeyFn>,
    /// The refusal status. `None` means 429.
    pub status: Option<StatusCode>,
}

/// A process-local limiter. Buckets are never evicted: fine behind Traefik with a bounded user
/// base, not for keys taken from a hostile public range.
pub struct RateLimiter {
    limit: f64,
    burst: f64,
    key: KeyFn,
    status: StatusCode,
    buckets: Mutex<HashMap<String, (f64, Instant)>>,
}

impl RateLimiter {
    /// Builds a limiter from `config`.
    pub fn new(config: RateLimitConfig) -> Self {
        let limit = if config.limit > 0.0 { config.limit } else { DEFAULT_RATE_LIMIT };
        let burst = if config.burst > 0 { f64::from(config.burst) } else { limit.ceil() };
        Self {
            limit,
            burst,
            key: config.key.unwrap_or_else(|| Arc::new(remote_host)),
            status: config.status.unwrap_or(StatusCode::TOO_MANY_REQUESTS),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Spends one token from `key`'s bucket at `now`. On refusal, returns the seconds to wait.
    pub fn allow(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let (tokens, last) = buckets.entry(key.to_owned()).or_insert((self.burst, now));
        let elapsed = now.saturating_duration_since(*last).as_secs_f64();
        *tokens = self.burst.min(*tokens + elapsed * self.limit);
        *last = now;
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            return Ok(());
        }
        Err((((1.0 - *tokens) / self.limit).ceil() as u64).max(1))
    }
}

fn remote_host(request: &Request) -> String {
    let remote =
        request.extensions().get::<RemoteAddr>().map_or_else(|| peer_string(request.extensions()), |r| r.0.clone());
    crate::proxies::split_host(&remote).map_or(remote.clone(), str::to_owned)
}

/// The middleware: mount with `axum::middleware::from_fn_with_state(Arc::new(limiter), rate_limit)`.
pub async fn rate_limit(State(limiter): State<Arc<RateLimiter>>, request: Request, next: Next) -> Response {
    let key = (limiter.key)(&request);
    if let Err(wait) = limiter.allow(&key, Instant::now()) {
        let mut response = (limiter.status, RATE_LIMIT_EXCEEDED_BODY).into_response();
        let headers = response.headers_mut();
        headers.insert(header::RETRY_AFTER, HeaderValue::from(wait));
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
        return response;
    }
    next.run(request).await
}
