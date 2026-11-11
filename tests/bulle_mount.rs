#![cfg(feature = "bulle")]

#[path = "bulle/fake.rs"]
mod fake;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::Request;
use http::StatusCode;
use serde_json::Value;
use tower::ServiceExt;
use tronc::bulle::{Client, IdeasOptions, TokenFuture, TokenSource, mount_ideas};

use fake::{Answer, Fake, client, fake, fixed};

struct Failing;

impl TokenSource for Failing {
    fn token(&self) -> TokenFuture<'_> {
        Box::pin(std::future::ready(Err(tronc::Error::failed("Sign in again."))))
    }
}

struct App {
    router: Router,
    bulle: Fake,
}

async fn app(answer: Answer, ai: bool, failing: bool) -> App {
    let bulle = fake(answer, Duration::ZERO).await;
    let mut options = IdeasOptions::new(
        "agenda",
        client(&bulle),
        Arc::new(move |_| if failing { Box::new(Failing) } else { Box::new("tok".to_owned()) }),
    );
    options.space =
        Some(Arc::new(|parts| parts.headers.get("x-space").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned()));
    options.ai_access = Some(Arc::new(move |_| ai));
    options.upgrade_url = "https://sso.example/spaces".into();
    let router = Router::new().nest("/api", mount_ideas(Router::new(), options).expect("mount"));
    App { router, bulle }
}

impl App {
    async fn call(&self, method: &str, path: &str, body: &str) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header("x-space", "team-1")
            .body(Body::from(body.to_owned()))
            .expect("request");
        let response = self.router.clone().oneshot(request).await.expect("response");
        let status = response.status();
        let raw = to_bytes(response.into_body(), 1 << 20).await.expect("body");
        (status, serde_json::from_slice(&raw).unwrap_or(Value::Null))
    }
}

fn code(body: &Value) -> &str {
    body["error"]["code"].as_str().unwrap_or("")
}

#[tokio::test]
async fn mounted_list_scopes_to_app_and_space() {
    let a = app(fixed(200, r#"{"ideas":[{"id":"i1"}],"generating":false}"#), true, false).await;
    let (status, body) = a.call("GET", "/api/ideas", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ai_access"], true);
    assert!(body.get("upgrade_url").is_none());
    assert_eq!(body["ideas"].as_array().map(Vec::len), Some(1));
    let seen = a.bulle.seen();
    assert_eq!((seen.method.as_str(), seen.query.as_str()), ("GET", "app=agenda&space_id=team-1"));
    assert_eq!((seen.auth.as_str(), seen.body.as_str()), ("Bearer tok", ""));
    let (status, _) = a.call("POST", "/api/ideas/a%2Fb/run", "").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(a.bulle.seen().path, "/api/ideas/a%2Fb/run");
}

#[tokio::test]
async fn mounted_free_plan_never_calls_bulle() {
    let a = app(fixed(200, "{}"), false, false).await;
    let (status, body) = a.call("GET", "/api/ideas", "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ai_access"], false);
    assert_eq!(body["upgrade_url"], "https://sso.example/spaces");
    assert_eq!(body["ideas"], serde_json::json!([]));
    for (path, body) in [
        ("/api/ideas/generate", ""),
        ("/api/ideas/i1/run", ""),
        ("/api/ideas/runs/r1/approvals/c1", r#"{"approve":true}"#),
    ] {
        let (status, answer) = a.call("POST", path, body).await;
        assert_eq!((status, code(&answer)), (StatusCode::PAYMENT_REQUIRED, "payment_required"), "{path}");
        assert_eq!(answer["error"]["upgrade_url"], "https://sso.example/spaces");
    }
    assert_eq!(a.bulle.seen().requests, 0, "a free plan must not reach Bulle");
    let (status, _) = a.call("POST", "/api/ideas/runs/r1/approvals/c1", r#"{"approve":false}"#).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "declining needs no AI access");
}

#[tokio::test]
async fn mounted_errors_map_for_the_frontend() {
    let cases = [
        (429, r#"{"error":{"code":"rate_limited","message":"at most 3"}}"#, 429, "rate_limited"),
        (409, r#"{"error":{"code":"already_exists","message":"busy"}}"#, 409, "already_exists"),
        (
            402,
            r#"{"error":{"code":"payment_required","message":"no","upgrade_url":"/spaces"}}"#,
            402,
            "payment_required",
        ),
        (401, "{}", 503, "unavailable"),
        (500, "{}", 503, "unavailable"),
    ];
    for (bulle_status, bulle_body, want, want_code) in cases {
        let a = app(fixed(bulle_status, bulle_body), true, false).await;
        let (status, body) = a.call("POST", "/api/ideas/generate", "").await;
        assert_eq!((status.as_u16(), code(&body)), (want, want_code), "bulle {bulle_status}");
    }
    let a = app(fixed(200, "{}"), true, true).await;
    let (status, body) = a.call("GET", "/api/ideas", "").await;
    assert_eq!((status.as_u16(), code(&body)), (412, "failed_precondition"));
    assert!(
        mount_ideas(
            Router::<()>::new(),
            IdeasOptions::new("Bad App", client(&a.bulle), Arc::new(|_| Box::new(String::new())))
        )
        .is_err()
    );
}

#[test]
fn the_url_must_be_an_origin() {
    for bad in ["", "bulle.example", "ftp://bulle.example", "https://u:p@bulle.example", "https://b.example/?q"] {
        assert!(Client::new(bad).is_err(), "{bad:?}");
    }
    let c = Client::new("https://bulle.example/").expect("client");
    assert_eq!(c.continue_url(""), "https://bulle.example");
}
