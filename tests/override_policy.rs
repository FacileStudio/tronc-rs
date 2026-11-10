#![cfg(feature = "http")]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use http::Request;
use tower::ServiceExt;
use tronc::http::{Config, SecurityOverride, SecurityOverrideConfig, layers, security_override};

fn embed(config: SecurityOverrideConfig) -> Router {
    let route = from_fn_with_state(Arc::new(SecurityOverride::new(config).expect("override")), security_override);
    Router::new()
        .route("/embed", get(|| async { "embed" }).layer(route))
        .route("/page", get(|| async { "page" }))
        .layer(layers(Config::default()).expect("chain"))
}

async fn headers(router: Router, path: &str) -> http::HeaderMap {
    router
        .oneshot(Request::get(path).body(Body::empty()).expect("request"))
        .await
        .expect("infallible")
        .headers()
        .clone()
}

#[tokio::test]
async fn opening_frame_ancestors_extends_the_chassis_policy_on_that_route_only() {
    let mut sources = tronc::http::Sources::new();
    sources.insert("frame-ancestors".into(), vec!["https://bulle.facile.studio".into()]);
    let router =
        embed(SecurityOverrideConfig { csp_sources: sources, allow: vec!["camera".into()], ..Default::default() });
    let embed = headers(router.clone(), "/embed").await;
    assert_eq!(
        embed["content-security-policy"],
        "frame-ancestors https://bulle.facile.studio; object-src 'none'; base-uri 'self'"
    );
    assert!(
        embed["content-security-policy-report-only"]
            .to_str()
            .unwrap_or("")
            .contains("frame-ancestors https://bulle.facile.studio;")
    );
    assert!(!embed.contains_key("x-frame-options"));
    assert_eq!(embed["permissions-policy"], "camera=(self), microphone=(), geolocation=()");
    let page = headers(router, "/page").await;
    assert_eq!(page["x-frame-options"], "DENY");
}

#[tokio::test]
async fn an_override_can_promote_report_only_to_enforced() {
    let router = embed(SecurityOverrideConfig { csp_enforce: Some(true), ..Default::default() });
    let embed = headers(router, "/embed").await;
    assert!(!embed.contains_key("content-security-policy-report-only"));
    assert!(embed["content-security-policy"].to_str().unwrap_or("").starts_with("default-src 'self';"));
}

#[test]
fn an_override_refuses_what_the_chassis_refuses() {
    let bad = SecurityOverrideConfig { allow: vec!["camera=*".into()], ..Default::default() };
    assert!(SecurityOverride::new(bad).is_err());
}
