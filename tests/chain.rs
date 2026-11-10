#![cfg(feature = "http")]

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;
use tronc::http::{Config, RequestId, layers};

fn app(config: Config) -> Router {
    Router::new()
        .route("/id", get(|RequestId(id): RequestId| async move { id }))
        .route("/panic", get(boom))
        .layer(layers(config).expect("chain"))
}

async fn send(router: Router, request: Request<Body>) -> (StatusCode, http::HeaderMap, String) {
    let response = router.oneshot(request).await.expect("infallible");
    let (parts, body) = response.into_parts();
    let body = body.collect().await.expect("body").to_bytes();
    (parts.status, parts.headers, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn a_well_formed_request_id_is_kept_and_echoed() {
    let request = Request::get("/id").header("x-request-id", "abc-123:/x.y_z").body(Body::empty()).expect("request");
    let (_, headers, body) = send(app(Config::default()), request).await;
    assert_eq!(body, "abc-123:/x.y_z");
    assert_eq!(headers["x-request-id"], "abc-123:/x.y_z");
}

#[tokio::test]
async fn a_hostile_request_id_is_replaced_with_an_opaque_one() {
    for hostile in ["has space", "<script>", &"a".repeat(129)] {
        let request = Request::get("/id").header("x-request-id", hostile).body(Body::empty()).expect("request");
        let (_, headers, body) = send(app(Config::default()), request).await;
        assert_eq!(body.len(), 26, "{hostile:?} was kept");
        assert!(body.bytes().all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b)));
        assert_eq!(headers["x-request-id"], body.as_str());
    }
}

#[tokio::test]
async fn a_panic_answers_the_internal_envelope_with_headers() {
    let (status, headers, body) =
        send(app(Config::default()), Request::get("/panic").body(Body::empty()).expect("request")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body, "{\"error\":{\"code\":\"internal\",\"message\":\"internal server error\"}}\n");
    assert!(headers.contains_key("x-request-id") && headers.contains_key("x-frame-options"));
}

async fn boom() -> &'static str {
    panic!("boom")
}
