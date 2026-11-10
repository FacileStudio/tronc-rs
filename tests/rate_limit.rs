#![cfg(feature = "http")]

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use tower::ServiceExt;
use tronc::http::{RateLimitConfig, RateLimiter, rate_limit};

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
