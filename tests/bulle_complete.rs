#![cfg(feature = "bulle")]

#[path = "bulle/fake.rs"]
mod fake;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::response::IntoResponse;
use http::{StatusCode, header};
use serde_json::{Value, json};
use tronc::bulle::{CompleteRequest, Error};

use fake::{client, fake, fixed};

#[tokio::test]
async fn complete_sends_the_typed_body_and_reads_the_answer() {
    let f = fake(fixed(200, r#"{"json":{"ok":true},"model":"m1"}"#), Duration::ZERO).await;
    let request = CompleteRequest {
        prompt: "Group these notes".into(),
        input: "a, b".into(),
        schema: Some(json!({"type": "object"})),
        max_tokens: 512,
        space_id: "s1".into(),
        ..CompleteRequest::default()
    };
    let done = client(&f).complete("tok", &request).await.expect("complete");
    assert_eq!((done.json, done.model.as_str(), done.text.as_str()), (Some(json!({"ok": true})), "m1", ""));
    let seen = f.seen();
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("POST", "/api/ai/complete"));
    assert_eq!((seen.auth.as_str(), seen.query.as_str()), ("Bearer tok", ""));
    let body: Value = serde_json::from_str(&seen.body).expect("body");
    let want = json!({"prompt": "Group these notes", "input": "a, b", "schema": {"type": "object"}, "max_tokens": 512, "space_id": "s1"});
    assert_eq!(body, want, "empty fields stay out, so an older Bulle never sees space_id unset");
    let bare =
        serde_json::to_value(CompleteRequest { prompt: "p".into(), ..CompleteRequest::default() }).expect("json");
    assert_eq!(bare, json!({"prompt": "p"}));
}

#[tokio::test]
async fn a_redirect_is_never_followed() {
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let app = Router::new().fallback(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        async { (StatusCode::FOUND, [(header::LOCATION, "/elsewhere")]) }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, app).await });
    let bulle = tronc::bulle::Client::new(&url).expect("client");
    let failed = bulle.complete("tok", &CompleteRequest::default()).await.expect_err("redirect");
    assert!(failed.is_unreachable(), "{failed}");
    assert_eq!(hits.load(Ordering::SeqCst), 1, "the token never follows a redirect");
}

#[tokio::test]
async fn a_plan_refusal_keeps_its_key_in_the_envelope() {
    let body = r#"{"error":{"code":"payment_required","message":"No AI","upgrade_url":"javascript:alert(1)","key":"ai.access"}}"#;
    let f = fake(fixed(402, body), Duration::ZERO).await;
    let failed: Error = client(&f).complete("tok", &CompleteRequest::default()).await.expect_err("402");
    let response = tronc::Error::from(failed).into_response();
    assert_eq!(response.status(), 402);
    let raw = axum::body::to_bytes(response.into_body(), 1 << 10).await.expect("body");
    let envelope: Value = serde_json::from_slice(&raw).expect("envelope");
    assert_eq!(envelope, json!({"error": {"code": "payment_required", "message": "No AI", "key": "ai.access"}}));
}
