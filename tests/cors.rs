#![cfg(feature = "http")]

use axum::Router;
use axum::body::Body;
use axum::routing::get;
use http::{Request, StatusCode};
use tower::ServiceExt;
use tronc::http::{Config, CorsConfig, layers};

fn app(config: Config) -> Router {
    Router::new().route("/id", get(|| async { "id" })).layer(layers(config).expect("chain"))
}

async fn send(router: Router, request: Request<Body>) -> (StatusCode, http::HeaderMap, ()) {
    let response = router.oneshot(request).await.expect("infallible");
    (response.status(), response.headers().clone(), ())
}

fn cors(origins: &[&str], credentials: bool) -> Config {
    let cors = CorsConfig {
        allowed_origins: origins.iter().map(|o| (*o).to_owned()).collect(),
        allow_credentials: credentials,
        ..CorsConfig::default()
    };
    Config { cors, ..Config::default() }
}

#[tokio::test]
async fn cors_answers_allowed_preflights_and_refuses_the_rest() {
    let preflight =
        |origin: &str| Request::options("/id").header("origin", origin).body(Body::empty()).expect("request");
    let (status, headers, _) = send(app(cors(&["https://a.example"], true)), preflight("https://a.example")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(headers["access-control-allow-origin"], "https://a.example");
    assert_eq!(headers["access-control-allow-credentials"], "true");
    assert_eq!(headers["access-control-expose-headers"], "X-Request-Id");
    assert_eq!(headers.get_all("vary").iter().count(), 3);
    let (status, headers, _) = send(app(cors(&["https://a.example"], true)), preflight("https://evil.example")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!headers.contains_key("access-control-allow-origin"));
}

#[test]
fn a_credentialed_wildcard_is_refused_at_startup() {
    assert!(layers(cors(&["*"], true)).is_err_and(|e| e.contains("lets any site read authenticated responses")));
}

#[tokio::test]
async fn a_same_origin_request_passes_untouched() {
    let (status, headers, _) =
        send(app(cors(&["https://a.example"], false)), Request::get("/id").body(Body::empty()).expect("request")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!headers.contains_key("access-control-allow-origin"));
}
