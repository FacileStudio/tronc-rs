#![cfg(feature = "bulle")]

#[path = "bulle/fake.rs"]
mod fake;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tronc::bulle::{self, Client, Error, ForgetFuture, STATE_NEEDS_APPROVAL, TokenFuture, TokenSource};

use fake::{Answer, client, fake, fixed};

#[tokio::test]
async fn ideas_send_app_space_and_token() {
    let body = r#"{"ideas":[{"id":"i1","kind":"task","title":"T","extra":1,"sources":null,"created_at":"2026-11-10T08:00:00Z"}],"generating":true,"goals":[]}"#;
    let f = fake(fixed(200, body), Duration::ZERO).await;
    let list = client(&f).ideas("tok", "agenda", "space 1").await.expect("ideas");
    let seen = f.seen();
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("GET", "/api/ideas"));
    assert_eq!(seen.query, "app=agenda&space_id=space+1");
    assert_eq!(seen.auth, "Bearer tok");
    assert_eq!(list.ideas.len(), 1);
    assert_eq!(list.ideas[0].id, "i1");
    assert!(list.ideas[0].sources.is_empty() && list.ideas[0].created_at.is_some());
    assert!(list.generating);
}

#[tokio::test]
async fn payment_required_reads_bare_and_enveloped_bodies() {
    for body in [
        r#"{"error":{"code":"payment_required","message":"No AI","upgrade_url":"https://sso.example/spaces","key":"ai.access"}}"#,
        r#"{"code":"payment_required","message":"No AI","upgrade_url":"https://sso.example/spaces","key":"ai.access"}"#,
    ] {
        let f = fake(fixed(402, body), Duration::ZERO).await;
        match client(&f).generate("tok", "opus", "").await {
            Err(Error::PlanRequired(plan)) => {
                assert_eq!(plan.message, "No AI");
                assert_eq!(plan.upgrade_url, "https://sso.example/spaces");
                assert_eq!(plan.key, "ai.access");
            }
            other => panic!("{body}: {other:?}"),
        }
    }
    let f = fake(fixed(402, r#"{"error":{"message":"x","upgrade_url":"javascript:alert(1)"}}"#), Duration::ZERO).await;
    match client(&f).generate("tok", "opus", "").await {
        Err(Error::PlanRequired(plan)) => assert_eq!(plan.upgrade_url, "", "an unsafe upgrade_url is dropped"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn statuses_keep_their_meaning() {
    let f = fake(fixed(429, r#"{"error":{"code":"rate_limited","message":"at most 3"}}"#), Duration::ZERO).await;
    match client(&f).generate("tok", "opus", "").await {
        Err(Error::Status { status: 429, message, .. }) => assert_eq!(message, "at most 3"),
        other => panic!("429: {other:?}"),
    }
    let f = fake(fixed(502, "oops"), Duration::ZERO).await;
    assert!(client(&f).ideas("tok", "opus", "").await.is_err_and(|e| e.is_unreachable()));
    let f = fake(fixed(200, "{not json"), Duration::ZERO).await;
    assert!(client(&f).ideas("tok", "opus", "").await.is_err_and(|e| e.is_unreachable()));
    let slow = fake(fixed(200, "{}"), Duration::from_millis(300)).await;
    let http = reqwest::Client::builder().timeout(Duration::from_millis(30)).build().expect("http");
    let impatient = Client::with_http(&slow.url, http).expect("client");
    assert!(impatient.ideas("tok", "opus", "").await.is_err_and(|e| e.is_unreachable()));
    let closed = Client::new("http://127.0.0.1:9").expect("client");
    assert!(closed.ideas("tok", "opus", "").await.is_err_and(|e| e.is_unreachable()));
}

#[tokio::test]
async fn requests_carry_escaped_paths_and_bodies() {
    let f = fake(fixed(202, r#"{"run_id":"r1"}"#), Duration::ZERO).await;
    let c = client(&f);
    let start = c.run("tok", "a/../b?x=1", "s").await.expect("run");
    assert_eq!(start.run_id, "r1");
    let seen = f.seen();
    assert_eq!(seen.path, "/api/ideas/a%2F..%2Fb%3Fx=1/run");
    assert_eq!(seen.query, "space_id=s");
    for bad in ["", ".", ".."] {
        assert!(matches!(c.decide("tok", bad, "c", true).await, Err(Error::BadSegment)), "segment {bad:?}");
    }
    assert!(matches!(c.dismiss("tok", &"x".repeat(201)).await, Err(Error::BadSegment)));
    assert_eq!(f.seen().requests, 1, "a bad segment never reaches Bulle");
    let f = fake(fixed(204, ""), Duration::ZERO).await;
    let c = client(&f);
    c.decide("tok", "r1", "c1", false).await.expect("decide");
    let seen = f.seen();
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("POST", "/api/runs/r1/approvals/c1"));
    assert_eq!(serde_json::from_str::<Value>(&seen.body).expect("json"), serde_json::json!({ "approve": false }));
    c.dismiss("tok", "i1").await.expect("dismiss");
    let seen = f.seen();
    assert_eq!((seen.method.as_str(), seen.body.as_str()), ("PATCH", r#"{"status":"dismissed"}"#));
    c.generate("tok", "opus", "").await.ok();
    assert_eq!((f.seen().query.as_str(), f.seen().body.as_str()), ("app=opus", r#"{"space_id":""}"#));
}

#[tokio::test]
async fn run_status_derives_state_and_links() {
    let body = r#"{"id":"r1","session_id":"s 1","status":"running","outputs":[{"url":"javascript:x"},{"url":"https://a.example/x"}],"pending_approvals":[{"call_id":"c1","args":{"to":"x"}}]}"#;
    let f = fake(fixed(200, body), Duration::ZERO).await;
    let run = client(&f).run_status("tok", "r1").await.expect("run");
    assert_eq!(run.state, STATE_NEEDS_APPROVAL);
    assert_eq!(run.continue_url, format!("{}/s/s%201", f.url));
    assert_eq!(run.outputs[0].url, "");
    assert_eq!(run.outputs[1].url, "https://a.example/x");
    assert_eq!(run.pending_approvals[0].args["to"], "x");
    assert_eq!(bulle::run_state("finished", 0), bulle::STATE_DONE);
    assert_eq!(bulle::run_state("interrupted", 0), bulle::STATE_FAILED);
    assert_eq!(bulle::run_state("queued", 0), bulle::STATE_RUNNING);
}

struct Rotating {
    forgot: Mutex<bool>,
}

impl TokenSource for Rotating {
    fn token(&self) -> TokenFuture<'_> {
        let token = if *self.forgot.lock().expect("lock") { "fresh" } else { "stale" };
        Box::pin(std::future::ready(Ok(token.to_owned())))
    }

    fn forget(&self) -> Option<ForgetFuture<'_>> {
        *self.forgot.lock().expect("lock") = true;
        Some(Box::pin(std::future::ready(())))
    }
}

#[tokio::test]
async fn a_401_retries_once_after_forget() {
    let answer: Answer = Arc::new(|n| if n == 1 { (401, "{}".into()) } else { (200, r#"{"ideas":[]}"#.into()) });
    let f = fake(answer, Duration::ZERO).await;
    let tokens = Rotating { forgot: Mutex::new(false) };
    client(&f).ideas(&tokens, "opus", "").await.expect("retried");
    assert_eq!((f.seen().requests, f.seen().auth.as_str()), (2, "Bearer fresh"));
    let f = fake(fixed(401, "{}"), Duration::ZERO).await;
    assert!(matches!(client(&f).ideas("tok", "opus", "").await, Err(Error::Status { status: 401, .. })));
    assert_eq!(f.seen().requests, 1, "a fixed token never retries");
}
