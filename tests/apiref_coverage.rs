#![cfg(all(feature = "apiref", feature = "health"))]

#[path = "apiref/fixture.rs"]
mod fixture;

use axum::Router;
use axum::routing::{get, post};
use tronc::apiref;

use fixture::{app, config};

#[tokio::test]
async fn a_route_mounted_outside_the_registry_is_reported() {
    let extra = Router::new().route("/secret/{id}", post(|| async { "x" }).get(|| async { "y" }));
    let (router, doc) = app(extra);
    let missing = apiref::undocumented(&router, &doc, &config(), &[]);
    assert_eq!(missing, ["GET /api/secret/{id}", "POST /api/secret/{id}"]);
}

#[tokio::test]
async fn a_fully_documented_app_reports_nothing_and_ignore_prefixes_hold() {
    let (router, doc) = app(Router::new());
    assert!(apiref::undocumented(&router, &doc, &config(), &[]).is_empty());
    let extra = Router::new().route("/webhooks/stripe", post(|| async { "ok" }));
    let (router, doc) = app(extra);
    assert!(apiref::undocumented(&router, &doc, &config(), &["/api/webhooks"]).is_empty());
}

#[test]
fn mounted_routes_are_read_from_the_router() {
    let router: Router = Router::new()
        .route("/a", get(|| async {}))
        .route("/b/{id}", get(|| async {}).post(|| async {}).delete(|| async {}))
        .route("/c", axum::routing::any(|| async {}))
        .fallback(|| async {});
    let mounted = apiref::mounted(&router);
    let listed: Vec<(&str, Vec<&str>)> =
        mounted.iter().map(|(p, m)| (p.as_str(), m.iter().map(String::as_str).collect())).collect();
    assert_eq!(listed, [("/a", vec!["GET"]), ("/b/{id}", vec!["GET", "POST", "DELETE"]), ("/c", vec!["ANY"])]);
}
