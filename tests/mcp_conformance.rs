#![cfg(feature = "mcptest")]

#[path = "mcp/app.rs"]
mod app;
#[path = "mcp/notes.rs"]
mod notes;

use std::sync::Arc;

use tronc::mcp::{Caller, GONE, STATE_READY, is_changed};
use tronc::mcptest::{self, ClientOptions, Config, ConfirmCase, Target, call, connect, decode};

use app::serve;
use notes::{DeleteOutput, Notes};

fn token(scopes: &[&str]) -> String {
    let scopes = if scopes.is_empty() { "notes.read,notes.write".to_owned() } else { scopes.join(",") };
    format!("u1|{scopes}")
}

async fn ask_by_link(url: &str, id: &str) -> String {
    let client = connect(url, "u1|notes.read,notes.write", ClientOptions::default()).await;
    let args = serde_json::from_value(serde_json::json!({ "id": id })).expect("args");
    let out: DeleteOutput = decode(&call(&client, "notes_delete", args).await);
    out.confirm_url.rsplit('=').next().expect("action id").to_owned()
}

pub fn fresh(notes: &Arc<Notes>) -> ConfirmCase {
    let notes = notes.clone();
    let setup = move || -> mcptest::Fut<Target> {
        let id = {
            let mut text = notes.text.lock().expect("lock");
            let id = format!("n{}", text.len() + notes.deleted.lock().expect("lock").len());
            text.insert(id.clone(), "hello".to_owned());
            id
        };
        let counter = notes.clone();
        let args = serde_json::from_value(serde_json::json!({ "id": id })).expect("args");
        let runs = move || -> mcptest::Fut<usize> { Box::pin(std::future::ready(counter.deletes(&id))) };
        Box::pin(std::future::ready(Target { args, runs: Arc::new(runs) }))
    };
    ConfirmCase { tool: "notes_delete".into(), setup: Arc::new(setup) }
}

fn person(user_id: &str) -> Caller {
    Caller { user_id: user_id.into(), ..Caller::default() }
}

#[tokio::test]
async fn the_toy_app_passes_the_conformance_kit() {
    let app = serve(false).await;
    let confirm = vec![fresh(&app.notes)];
    mcptest::run(&Config { url: app.url.clone(), token: Arc::new(token), confirm }).await;
}

#[tokio::test]
async fn a_read_only_server_passes_the_conformance_kit() {
    let app = serve(true).await;
    let token = Arc::new(|_: &[&str]| "u1|notes.read".to_owned());
    mcptest::run(&Config { url: app.url.clone(), token, confirm: Vec::new() }).await;
}

#[tokio::test]
async fn a_target_changed_after_the_question_is_not_run() {
    let app = serve(false).await;
    app.notes.set("a", "before");
    let id = ask_by_link(&app.url, "a").await;
    app.notes.set("a", "after");
    let changed = app.confirm.confirm(&person("u1"), &id).await.expect_err("changed");
    assert!(is_changed(&changed) && app.notes.deletes("a") == 0, "want CHANGED and no delete, got {changed}");
    let again = app.confirm.confirm(&person("u1"), &id).await.expect_err("closed");
    assert_eq!(again.message, GONE, "a stale action is closed");
}

#[tokio::test]
async fn only_the_owner_confirms_once_from_the_confirm_page() {
    let app = serve(false).await;
    app.notes.set("a", "text");
    let id = ask_by_link(&app.url, "a").await;
    let stranger = app.confirm.confirm(&person("u2"), &id).await.expect_err("stranger");
    assert_eq!(stranger.message, GONE, "another person must not confirm");
    let dismissed = app.confirm.dismiss(&person("u2"), &id).await.expect_err("stranger");
    assert_eq!(dismissed.message, GONE, "another person must not dismiss");
    let open = app.confirm.open("u1").await.expect("open");
    assert_eq!(open.len(), 1, "want one waiting action");
    assert_eq!(app.confirm.state(&open[0]).await, STATE_READY);
    app.confirm.confirm(&person("u1"), &id).await.expect("the owner confirms");
    assert_eq!(app.notes.deletes("a"), 1);
}
