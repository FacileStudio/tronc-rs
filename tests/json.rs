#![cfg(feature = "http")]

use axum::Router;
use axum::body::Body;
use axum::routing::post;
use http::{Request, StatusCode};
use serde::Deserialize;
use tower::ServiceExt;
use tronc::json::{Json, JsonLimit, assert_denies_unknown_fields, encode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Strict {
    name: String,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Loose {
    name: String,
}

#[test]
fn the_unknown_field_guard_accepts_a_strict_type() {
    assert_denies_unknown_fields::<Strict>();
}

#[test]
#[should_panic(expected = "accepts unknown fields")]
fn the_unknown_field_guard_catches_a_loose_type() {
    assert_denies_unknown_fields::<Loose>();
}

#[test]
fn encoding_matches_gos_encoder() {
    assert_eq!(
        encode(&serde_json::json!({ "b": "<&>\u{2028}", "a": 1 })),
        b"{\"a\":1,\"b\":\"\\u003c\\u0026\\u003e\\u2028\"}\n"
    );
}

#[tokio::test]
async fn a_route_can_raise_its_own_cap() {
    let router = Router::new()
        .route("/small", post(|Json(v): Json<serde_json::Value>| async move { Json(v) }))
        .route("/big", post(|JsonLimit(v): JsonLimit<serde_json::Value, { 4 << 20 }>| async move { Json(v) }));
    let body = format!("[\"{}\"]", "a".repeat(2 << 20));
    let status = |path: &'static str| {
        let router = router.clone();
        let body = body.clone();
        async move {
            router.oneshot(Request::post(path).body(Body::from(body)).expect("request")).await.expect("ok").status()
        }
    };
    assert_eq!(status("/small").await, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(status("/big").await, StatusCode::OK);
}
