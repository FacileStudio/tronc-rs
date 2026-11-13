use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;
use tronc::mcp::{
    self, Action, Annotations, Answer, AppConfig, AppSpec, Call, ConfirmConfig, Confirmation, Confirmations, Kind,
    Scopes, Tool, ToolError, ToolSpec, state_key,
};
use tronc::mcptest::MemoryPending;

use super::notes::{NoteInput, NoteList, NoteRef, Notes, Nothing, Tokens};

pub struct App {
    pub notes: Arc<Notes>,
    pub store: Arc<MemoryPending>,
    pub confirm: Arc<Confirmations>,
    pub url: String,
}

fn spec(name: &str, title: &str, annotations: Annotations) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        title: title.into(),
        description: format!("{title}."),
        annotations: Some(annotations),
    }
}

async fn delete(confirm: Confirmations, call: Call, input: NoteRef) -> Result<Answer<Confirmation>, ToolError> {
    if call.retrying() {
        return Ok(confirm.answered(&call, "delete", "The user confirmed and the note was deleted.").await?.into());
    }
    confirm.ask(&call, "delete", &input.id, &format!("Delete note {}?", input.id)).await
}

fn list(notes: &Arc<Notes>) -> Tool {
    let lister = notes.clone();
    let list = move |_call: Call, _input: Nothing| {
        let ids = lister.text.lock().expect("lock").keys().cloned().collect();
        async move { Ok(Answer::Output(NoteList { ids })) }
    };
    mcp::tool(spec("notes_list", "List notes", Annotations::READ), list)
}

fn tools(notes: &Arc<Notes>, confirm: &Confirmations) -> Vec<Tool> {
    let (writer, confirm) = (notes.clone(), confirm.clone());
    let write = move |_call: Call, input: NoteInput| {
        writer.set(&input.id, &input.text);
        async move { Ok(Answer::Output(NoteList { ids: vec![input.id] })) }
    };
    vec![
        mcp::tool(spec("notes_write", "Write a note", Annotations::OVERWRITE), write),
        list(notes),
        mcp::tool(spec("notes_delete", "Delete a note", Annotations::REMOVE), move |call, input| {
            delete(confirm.clone(), call, input)
        }),
    ]
}

fn kinds(notes: &Arc<Notes>) -> HashMap<String, Kind> {
    let (reader, runner) = (notes.clone(), notes.clone());
    let fingerprint = move |action: Action| std::future::ready(reader.fingerprint(&action));
    let run = move |action: Action| {
        runner.remove(&action);
        std::future::ready(Ok(()))
    };
    HashMap::from([("delete".to_owned(), Kind::new("notes_delete", fingerprint, run))])
}

async fn listen() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    (listener, base)
}

pub async fn replica(app: &App, read_only: bool) -> String {
    let (listener, base) = listen().await;
    let mut cfg = AppConfig::new(format!("{base}/mcp"), "https://issuer.example", Arc::new(Tokens));
    cfg.state_key = state_key("notes", "secret");
    cfg.pending = Some(app.store.clone());
    let mut spec = AppSpec { name: "notes".into(), title: "Notes".into(), ..AppSpec::default() };
    if read_only {
        spec.scopes = Scopes { read: "notes.read".into(), write: String::new() };
    } else {
        spec.kinds = kinds(&app.notes);
    }
    let mut server = mcp::App::new(cfg, spec).expect("app");
    assert_eq!(server.url, base, "the app URL is the resource's origin");
    server.options.tools = match &server.confirmations {
        Some(confirm) => tools(&app.notes, confirm),
        None => vec![list(&app.notes)],
    };
    let router = server.mount(Router::new()).expect("mount");
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("{base}/mcp")
}

pub async fn serve(read_only: bool) -> App {
    let notes = Arc::new(Notes::default());
    let store = Arc::new(MemoryPending::new());
    let cfg = ConfirmConfig {
        store: store.clone(),
        key: state_key("notes", "secret"),
        confirm_url: "https://notes.example/pending".to_owned(),
        kinds: kinds(&notes),
        pending_ttl: Duration::ZERO,
        ask_ttl: Duration::ZERO,
    };
    let confirm = Arc::new(Confirmations::new(cfg).expect("confirmations"));
    let mut app = App { notes, store, confirm, url: String::new() };
    app.url = replica(&app, read_only).await;
    app
}
