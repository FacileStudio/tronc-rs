#![cfg(feature = "mcptest")]

#[path = "mcp/app.rs"]
mod app;
#[path = "mcp/notes.rs"]
mod notes;

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResponse, ElicitationAction, JsonObject};
use tronc::mcp::CONFIRM_INPUT;
use tronc::mcptest::{ClientOptions, answering, call_once, connect, post};

use app::{replica, serve};

const TOKEN: &str = "u1|notes.read,notes.write";

fn note(id: &str) -> JsonObject {
    serde_json::from_value(serde_json::json!({ "id": id })).expect("args")
}

#[tokio::test]
async fn the_endpoint_hands_out_no_session() {
    let app = serve(false).await;
    let list = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;
    let (status, headers) = post(&app.url, TOKEN, list, &[]).await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(headers.get("mcp-session-id").is_none(), "a stateless endpoint never sets Mcp-Session-Id");
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"raw","version":"1"}}}"#;
    let (status, headers) = post(&app.url, TOKEN, init, &[]).await;
    assert_eq!(status, http::StatusCode::OK);
    assert!(headers.get("mcp-session-id").is_none(), "a legacy initialize gets no session either");
}

#[tokio::test]
async fn a_question_asked_on_one_replica_is_answered_on_another() {
    let app = serve(false).await;
    let other = replica(&app, false).await;
    app.notes.set("a", "text");
    let options = ClientOptions { elicit: Some(answering(ElicitationAction::Accept, Arc::default())), version: None };
    let first = connect(&app.url, TOKEN, options.clone()).await;
    let asked = call_once(&first, CallToolRequestParams::new("notes_delete").with_arguments(note("a"))).await;
    let CallToolResponse::InputRequired(question) = asked else { panic!("want input_required, got {asked:?}") };
    let second = connect(&other, TOKEN, options).await;
    let mut retry = CallToolRequestParams::new("notes_delete").with_arguments(note("a"));
    retry.request_state = question.request_state;
    let accept = serde_json::json!({ "action": "accept", "content": { CONFIRM_INPUT: true } });
    retry.input_responses = Some(BTreeMap::from([(CONFIRM_INPUT.to_owned(), accept)]));
    let done = call_once(&second, retry).await;
    assert!(matches!(&done, CallToolResponse::Complete(r) if r.is_error != Some(true)), "{done:?}");
    assert_eq!(app.notes.deletes("a"), 1);
}
