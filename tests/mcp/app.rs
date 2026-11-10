use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tokio::net::TcpListener;
use tronc::mcp::{
    self, Action, Annotations, Answer, Call, ConfirmConfig, Confirmations, Kind, Options, Scopes, Tool, ToolError,
    ToolSpec, failure,
};
use tronc::mcptest;

use super::notes::{DeleteOutput, NoteInput, NoteList, NoteRef, Notes, Nothing, Tokens};

pub struct App {
    pub notes: Arc<Notes>,
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

async fn delete(confirm: Arc<Confirmations>, call: Call, input: NoteRef) -> Result<Answer<DeleteOutput>, ToolError> {
    if call.retrying() {
        let outcome = confirm.resume(&call, "delete").await.map_err(|e| failure(e, "Call notes_delete again."))?;
        return Ok(DeleteOutput { status: outcome.status.into(), confirm_url: String::new() }.into());
    }
    let question = format!("Delete note {}?", input.id);
    let asked = confirm.request(&call, "delete", &input.id, &question).await;
    let (question, outcome) = asked.map_err(|e| failure(e, "Call notes_list for the note ids."))?;
    match question {
        Some(question) => Ok(Answer::InputRequired(question)),
        None => Ok(DeleteOutput { status: outcome.status.into(), confirm_url: outcome.confirm_url }.into()),
    }
}

fn tools(notes: &Arc<Notes>, confirm: &Arc<Confirmations>) -> Vec<Tool> {
    let (writer, lister, confirm) = (notes.clone(), notes.clone(), confirm.clone());
    let write = move |_call: Call, input: NoteInput| {
        writer.set(&input.id, &input.text);
        async move { Ok(Answer::Output(NoteList { ids: vec![input.id] })) }
    };
    let list = move |_call: Call, _input: Nothing| {
        let ids = lister.text.lock().expect("lock").keys().cloned().collect();
        async move { Ok(Answer::Output(NoteList { ids })) }
    };
    let read = Annotations { read_only: true, idempotent: true, ..Annotations::default() };
    let destroy = Annotations { destructive: true, idempotent: true, ..Annotations::default() };
    vec![
        mcp::tool(
            spec("notes_write", "Write a note", Annotations { idempotent: true, ..Annotations::default() }),
            write,
        ),
        mcp::tool(spec("notes_list", "List notes", read), list),
        mcp::tool(spec("notes_delete", "Delete a note", destroy), move |call, input| {
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
    let mut options = Options::new("notes", format!("{base}/mcp"), "https://issuer.example", Arc::new(Tokens));
    options.tools = tools(&app.notes, &app.confirm);
    if read_only {
        options.scopes = Scopes { read: "notes.read".into(), write: String::new() };
        options.tools = vec![options.tools.remove(1)];
    }
    let router = mcp::mount(Router::new(), options).expect("mount");
    tokio::spawn(async move { axum::serve(listener, router).await });
    format!("{base}/mcp")
}

pub async fn serve(read_only: bool) -> App {
    let notes = Arc::new(Notes::default());
    let cfg = ConfirmConfig {
        store: Arc::new(mcptest::MemoryPending::new()),
        key: b"key".to_vec(),
        confirm_url: "https://notes.example/pending".to_owned(),
        kinds: kinds(&notes),
        pending_ttl: Duration::ZERO,
        ask_ttl: Duration::ZERO,
    };
    let confirm = Arc::new(Confirmations::new(cfg).expect("confirmations"));
    let mut app = App { notes, confirm, url: String::new() };
    app.url = replica(&app, read_only).await;
    app
}
