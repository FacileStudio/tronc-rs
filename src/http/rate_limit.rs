use std::collections::HashMap;
use std::net::IpAddr;
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

/// Writes the answer to a refused request in place of the plain-text 429. `Retry-After` is set
/// on it afterwards.
pub type RefuseFn = Arc<dyn Fn(&Request) -> Response + Send + Sync>;

/// A per-key token bucket, as Go `middleware.RatelimitConfig`.
#[derive(Clone, Default)]
pub struct RateLimitConfig {
    /// Sustained requests per second. Zero or less means [`DEFAULT_RATE_LIMIT`].
    pub limit: f64,
    /// Requests allowed at once. Zero means `limit` rounded up.
    pub burst: u32,
    /// The bucket identity. `None` keys on the host of [`RemoteAddr`], so mount it inside
    /// [`super::layers`]. [`remote_addr_key64`] keys IPv6 callers on their /64.
    pub key: Option<KeyFn>,
    /// The refusal status. `None` means 429.
    pub status: Option<StatusCode>,
    /// Bounds the keys kept, as Go `MaxKeys`: once half of it is reached, the older half is
    /// dropped and a dropped key starts again with a full burst. Zero keeps every key forever,
    /// which suits a bounded set of keys, such as signed-in users, and not keys a hostile
    /// public range picks.
    pub max_keys: usize,
    /// Answers a refusal in place of the plain-text body, as Go `OnRefuse`.
    pub on_refuse: Option<RefuseFn>,
}

type Bucket = (f64, Instant);

/// A process-local limiter, as Go `middleware.Limiter`.
pub struct RateLimiter {
    limit: f64,
    burst: f64,
    key: KeyFn,
    status: StatusCode,
    on_refuse: Option<RefuseFn>,
    generation: usize,
    buckets: Mutex<(HashMap<String, Bucket>, HashMap<String, Bucket>)>,
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
            on_refuse: config.on_refuse,
            generation: if config.max_keys > 0 { config.max_keys.max(2) / 2 } else { 0 },
            buckets: Mutex::new((HashMap::new(), HashMap::new())),
        }
    }

    /// Spends one token from `key`'s bucket at `now`. On refusal, returns the seconds to wait.
    pub fn allow(&self, key: &str, now: Instant) -> Result<(), u64> {
        let mut buckets = self.buckets.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let (current, previous) = &mut *buckets;
        if !current.contains_key(key) {
            let bucket = previous.remove(key).unwrap_or((self.burst, now));
            if self.generation > 0 && current.len() >= self.generation {
                *previous = std::mem::take(current);
            }
            current.insert(key.to_owned(), bucket);
        }
        let (tokens, last) = current.get_mut(key).expect("the bucket was just inserted");
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

/// A [`KeyFn`] that buckets IPv6 callers by their /64 and IPv4 callers by their address, as Go
/// `middleware.RemoteAddrKey64`. One IPv6 subscriber usually holds a whole /64 and can pick a
/// fresh address for every request. Like the default key, it trusts [`RemoteAddr`]. A host that
/// is not an IP address is returned unchanged.
pub fn remote_addr_key64(request: &Request) -> String {
    prefix64(&remote_host(request))
}

fn prefix64(host: &str) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(v6)) => match v6.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let mut segments = v6.segments();
                segments[4..].fill(0);
                format!("{}/64", std::net::Ipv6Addr::from(segments))
            }
        },
        Ok(IpAddr::V4(v4)) => v4.to_string(),
        Err(_) => host.to_owned(),
    }
}

/// The middleware: mount with `axum::middleware::from_fn_with_state(Arc::new(limiter), rate_limit)`.
pub async fn rate_limit(State(limiter): State<Arc<RateLimiter>>, request: Request, next: Next) -> Response {
    let key = (limiter.key)(&request);
    let Err(wait) = limiter.allow(&key, Instant::now()) else {
        return next.run(request).await;
    };
    let mut response = match &limiter.on_refuse {
        Some(refuse) => refuse(&request),
        None => {
            let mut plain = (limiter.status, RATE_LIMIT_EXCEEDED_BODY).into_response();
            plain.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
            plain
        }
    };
    response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(wait));
    response
}
