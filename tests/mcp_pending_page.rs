#![cfg(feature = "mcptest")]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use http::request::Parts;
use http::{Request, StatusCode};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tower::ServiceExt;
use tronc::mcp::{
    Action, ConfirmConfig, Confirmations, Kind, OPEN_LIMIT, Pending, PendingItem, PendingPage, PendingResult, delegated,
};
use tronc::mcptest::MemoryPending;

fn action(id: &str, digest: &str) -> Action {
    let created_at = OffsetDateTime::now_utc();
    Action {
        id: id.into(),
        user_id: "u1".into(),
        kind: "delete".into(),
        target: id.into(),
        digest: digest.into(),
        summary: format!("Delete {id}?"),
        client_id: "claude".into(),
        created_at,
        expires_at: created_at + Duration::from_secs(3600),
    }
}

async fn user(parts: Parts) -> tronc::Result<String> {
    match parts.headers.get("x-user").and_then(|v| v.to_str().ok()) {
        Some("agent") => Err(delegated()),
        Some(user) => Ok(user.to_owned()),
        None => Ok(String::new()),
    }
}

fn confirmations(store: Arc<MemoryPending>) -> Confirmations {
    let kind = Kind::new("notes_delete", |_| async { Ok("now".to_owned()) }, |_| async { Ok(()) });
    Confirmations::new(ConfirmConfig {
        store,
        key: b"key".to_vec(),
        confirm_url: "https://notes.example/pending".into(),
        kinds: HashMap::from([("delete".to_owned(), kind)]),
        pending_ttl: Duration::ZERO,
        ask_ttl: Duration::ZERO,
    })
    .expect("confirmations")
}

async fn send(router: &Router, method: &str, path: &str, who: &str) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(path).header("x-user", who).body(Body::empty()).expect("req");
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), 1 << 20).await.expect("body");
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_page_serves_go_paths_and_json() {
    let store = Arc::new(MemoryPending::new());
    store.create(&action("a1", "now")).await.expect("create");
    store.create(&action("a2", "then")).await.expect("create");
    store.create(&action("a3", "now")).await.expect("create");
    let page = PendingPage::new(Some(confirmations(store.clone())), "notes-web", user);
    let router: Router = Router::new().nest("/api", page.routes());
    let (status, list) = send(&router, "GET", "/api/mcp/pending", "u1").await;
    assert_eq!(status, StatusCode::OK);
    let states: Vec<&str> =
        list["actions"].as_array().expect("actions").iter().map(|a| a["state"].as_str().unwrap_or("")).collect();
    assert_eq!(states.len(), 3, "{list}");
    assert!(states.contains(&"changed") && list.get("truncated").is_none(), "{list}");
    assert_eq!(list["actions"][0]["client_id"], "claude");
    let (status, body) = send(&router, "POST", "/api/mcp/pending/a1/dismiss", "u1").await;
    assert_eq!((status, body), (StatusCode::OK, json!({"status": "dismissed"})));
    let (status, body) = send(&router, "POST", "/api/mcp/pending/a3/confirm", "u1").await;
    assert_eq!((status, body), (StatusCode::OK, json!({"status": "done"})));
    let (status, body) = send(&router, "POST", "/api/mcp/pending/a2/confirm", "agent").await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("permission_denied")));
    let (status, _) = send(&router, "GET", "/api/mcp/pending", "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no person, no list");
    let (status, _) = send(&router, "POST", "/api/mcp/pending/a3/confirm", "u1").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a confirmation runs once");
}

#[tokio::test]
async fn the_page_describes_items_and_says_when_it_is_cut() {
    let store = Arc::new(MemoryPending::new());
    for n in 0..=OPEN_LIMIT {
        store.create(&action(&format!("b{n}"), "now")).await.expect("create");
    }
    let page = PendingPage::new(Some(confirmations(store)), "notes-web", user)
        .describe(|item: PendingItem, action: Action| async move { json!({"id": item.id, "note": action.target}) })
        .done(|_| async { PendingResult { status: "done".into(), url: "https://notes.example/n".into() } });
    let list = page.list("u1").await.expect("list");
    assert_eq!((list.actions.len(), list.truncated), (OPEN_LIMIT, true));
    assert_eq!(list.actions[0]["id"], list.actions[0]["note"]);
    let router: Router = page.routes();
    let (_, body) = send(&router, "POST", "/mcp/pending/b0/confirm", "u1").await;
    assert_eq!(body, json!({"status": "done", "url": "https://notes.example/n"}));
    let off: Router = PendingPage::new(None, "notes-web", user).routes();
    assert_eq!(send(&off, "GET", "/mcp/pending", "u1").await.0, StatusCode::NOT_FOUND, "MCP off answers 404");
}
