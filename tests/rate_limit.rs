#![cfg(feature = "http")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::response::IntoResponse;
use axum::routing::get;
use http::{Request, StatusCode};
use tower::ServiceExt;
use tronc::http::{RateLimitConfig, RateLimiter, RefuseFn, RemoteAddr, rate_limit, remote_addr_key64};

#[tokio::test]
async fn the_rate_limiter_refuses_past_the_burst() {
    let limiter = Arc::new(RateLimiter::new(RateLimitConfig { limit: 1.0, burst: 2, ..Default::default() }));
    let router = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(limiter, rate_limit));
    let statuses: Vec<StatusCode> = futures_statuses(router).await;
    assert_eq!(statuses, [StatusCode::OK, StatusCode::OK, StatusCode::TOO_MANY_REQUESTS]);
}

async fn futures_statuses(router: Router) -> Vec<StatusCode> {
    let mut statuses = Vec::new();
    for _ in 0..3 {
        statuses.push(
            router.clone().oneshot(Request::get("/").body(Body::empty()).expect("request")).await.expect("ok").status(),
        );
    }
    statuses
}

#[test]
fn a_bucket_refills_continuously_and_names_the_wait() {
    let limiter = RateLimiter::new(RateLimitConfig { limit: 2.0, burst: 1, ..Default::default() });
    let start = Instant::now();
    assert_eq!(limiter.allow("k", start), Ok(()));
    assert_eq!(limiter.allow("k", start), Err(1));
    assert_eq!(limiter.allow("other", start), Ok(()));
    assert_eq!(limiter.allow("k", start + Duration::from_millis(500)), Ok(()));
}

#[test]
fn max_keys_drops_the_oldest_keys_and_v6_callers_share_their_64() {
    let limiter = RateLimiter::new(RateLimitConfig { limit: 0.001, burst: 1, max_keys: 2, ..Default::default() });
    let now = Instant::now();
    assert_eq!(limiter.allow("a", now), Ok(()));
    assert!(limiter.allow("a", now).is_err(), "a spent its burst");
    for key in ["b", "c"] {
        assert_eq!(limiter.allow(key, now), Ok(()));
    }
    assert_eq!(limiter.allow("a", now), Ok(()), "a was dropped, so it starts again with a full burst");
    let key = |addr: &str| {
        let mut request = Request::get("/").body(Body::empty()).expect("request");
        request.extensions_mut().insert(RemoteAddr(addr.to_owned()));
        remote_addr_key64(&request)
    };
    assert_eq!(key("[2001:db8:1:2:aaaa::1]:443"), "2001:db8:1:2::/64");
    assert_eq!(key("[2001:db8:1:2:bbbb::9]:443"), key("[2001:db8:1:2:aaaa::1]:443"));
    assert_eq!(key("[::ffff:192.0.2.7]:80"), "192.0.2.7");
    assert_eq!(key("192.0.2.7:80"), "192.0.2.7");
}

#[tokio::test]
async fn on_refuse_writes_the_refusal_and_keeps_retry_after() {
    let refuse: RefuseFn = Arc::new(|_| (StatusCode::TOO_MANY_REQUESTS, "slow down").into_response());
    let config = RateLimitConfig { limit: 1.0, burst: 1, on_refuse: Some(refuse), ..Default::default() };
    let router = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(axum::middleware::from_fn_with_state(Arc::new(RateLimiter::new(config)), rate_limit));
    router.clone().oneshot(Request::get("/").body(Body::empty()).expect("request")).await.expect("ok");
    let refused = router.oneshot(Request::get("/").body(Body::empty()).expect("request")).await.expect("ok");
    assert_eq!(refused.headers().get("retry-after").map(|v| v.as_bytes()), Some(&b"1"[..]));
    let body = axum::body::to_bytes(refused.into_body(), 64).await.expect("body");
    assert_eq!(&body[..], b"slow down");
}
