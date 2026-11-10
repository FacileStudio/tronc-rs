#![cfg(all(feature = "apiref", feature = "health"))]

#[path = "apiref/fixture.rs"]
mod fixture;

use axum::Router;
use axum::body::Body;
use http::Request;
use http_body_util::BodyExt;
use tower::ServiceExt;
use tronc::apiref;

use fixture::app;

async fn fetch(router: Router, path: &str) -> (http::HeaderMap, String) {
    let response = router.oneshot(Request::get(path).body(Body::empty()).expect("request")).await.expect("ok");
    let headers = response.headers().clone();
    (headers, String::from_utf8_lossy(&response.into_body().collect().await.expect("body").to_bytes()).into_owned())
}

#[tokio::test]
async fn the_reference_page_loads_the_pinned_bundle_under_a_widened_csp() {
    let (router, _) = app(Router::new());
    let (headers, html) = fetch(router, "/docs").await;
    assert!(html.contains("<title>Feuilles API</title>") && html.contains(apiref::SCALAR_SCRIPT_URL));
    assert!(html.contains("data-url=\"/docs/openapi.json\""));
    let policy = headers["content-security-policy-report-only"].to_str().unwrap_or("");
    assert!(policy.contains(&format!("script-src 'self' {}", apiref::SCALAR_SCRIPT_URL)), "{policy}");
    assert!(policy.contains("font-src 'self' https://fonts.scalar.com"), "{policy}");
}

#[tokio::test]
async fn the_document_names_the_api_and_its_server() {
    let (router, _) = app(Router::new());
    let (headers, spec) = fetch(router, "/docs/openapi.json").await;
    assert_eq!(headers["cache-control"], "public, max-age=3600");
    let spec: serde_json::Value = serde_json::from_str(&spec).expect("json");
    assert_eq!(spec["info"]["title"], "Feuilles API");
    assert_eq!(spec["info"]["version"], apiref::DEFAULT_VERSION);
    assert_eq!(spec["servers"][0]["url"], "/api");
    assert!(spec["openapi"].as_str().is_some_and(|v| v.starts_with("3.1")));
    assert!(spec["paths"]["/sheets/{id}"]["get"].is_object());
}
