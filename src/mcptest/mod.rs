//! The conformance kit for servers built on [`crate::mcp`], a port of Go `tronc/mcp/mcptest`.
//! [`run`] checks a live endpoint the way MCP clients and the suite expect it: the 401
//! challenge naming the RFC 9728 metadata, the metadata itself, 405 on GET, the Origin check,
//! explicit annotations, object schemas, a stable sorted tool list, the 403 write-scope
//! challenge, and for each sensitive tool the whole confirmation path. [`MemoryPending`] is the
//! store an app's tests mount confirmations on.
//!
//! Checks panic with a message naming what failed, so they run inside any `#[tokio::test]`.

mod client;
mod confirm;
mod pending;
mod pending_store;
mod results;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use http::{StatusCode, header};
use rmcp::model::{JsonObject, Tool};
use serde::Deserialize;

pub use client::{Client, ClientOptions, Elicit, TestClient, connect, metadata_url_of, post};
pub use pending::MemoryPending;
pub use results::{answering, call, call_once, decode, text};

const LIST_TOOLS: &str = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#;

/// A boxed future, for the async callbacks of [`ConfirmCase`].
pub type Fut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Mints a valid access token for the endpoint with the given scopes, or with every scope the
/// metadata lists when none is given.
pub type TokenFn = Arc<dyn Fn(&[&str]) -> String + Send + Sync>;

/// Points [`run`] at a running endpoint.
#[derive(Clone)]
pub struct Config {
    /// The resource URI the endpoint serves.
    pub url: String,
    /// Mints tokens.
    pub token: TokenFn,
    /// The sensitive tools, and how to set up a fresh target for each run.
    pub confirm: Vec<ConfirmCase>,
}

/// A fresh target for one confirmation run.
#[derive(Clone)]
pub struct Target {
    /// The tool arguments that name it.
    pub args: JsonObject,
    /// Counts how many times the action really ran on it.
    pub runs: Arc<dyn Fn() -> Fut<usize> + Send + Sync>,
}

/// One sensitive tool under test.
#[derive(Clone)]
pub struct ConfirmCase {
    /// The tool name.
    pub tool: String,
    /// Creates a fresh target.
    pub setup: Arc<dyn Fn() -> Fut<Target> + Send + Sync>,
}

#[derive(Deserialize)]
struct Metadata {
    resource: String,
    authorization_servers: Vec<String>,
    scopes_supported: Vec<String>,
}

/// Checks the endpoint at `config.url`, panicking on the first failure.
pub async fn run(config: &Config) {
    let meta = check_challenge(config).await;
    let scopes = &meta.scopes_supported;
    assert!(
        (1..=2).contains(&scopes.len()),
        "metadata must list a read scope and, unless every tool is read-only, a write scope; got {scopes:?}"
    );
    let read = scopes[0].clone();
    let write = scopes.get(1).cloned().unwrap_or_default();
    let prefix = format!("{}_", read.split('.').next().unwrap_or(""));
    check_get(config).await;
    check_origin(config).await;
    let tools = listed(config).await;
    check_tools(&tools, &prefix);
    check_write_scope(config, &tools, &read, &write).await;
    for case in &config.confirm {
        confirm::check(config, case).await;
    }
}

async fn check_challenge(config: &Config) -> Metadata {
    let (status, headers) = post(&config.url, "", LIST_TOOLS, &[]).await;
    let challenge = headers.get(header::WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let metadata_url = metadata_url_of(&config.url);
    assert!(
        status == StatusCode::UNAUTHORIZED && challenge.contains(&format!("resource_metadata=\"{metadata_url}\"")),
        "no token: want 401 with resource_metadata={metadata_url:?}, got {status} {challenge:?}"
    );
    let document = reqwest::get(&metadata_url).await.unwrap_or_else(|e| panic!("metadata: {e}"));
    assert_eq!(document.status(), StatusCode::OK, "metadata: want a JSON document at {metadata_url}");
    let meta: Metadata = document.json().await.unwrap_or_else(|e| panic!("metadata: {e}"));
    assert!(
        meta.resource == config.url && !meta.authorization_servers.is_empty(),
        "metadata must name resource {} and an authorization server, got {} {:?}",
        config.url,
        meta.resource,
        meta.authorization_servers
    );
    meta
}

async fn check_get(config: &Config) {
    let response = reqwest::Client::new()
        .get(&config.url)
        .bearer_auth((config.token)(&[]))
        .header(header::ACCEPT, "text/event-stream")
        .send()
        .await
        .unwrap_or_else(|e| panic!("get: {e}"));
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "want 405 on GET");
}

async fn check_origin(config: &Config) {
    let token = (config.token)(&[]);
    let (status, _) = post(&config.url, &token, LIST_TOOLS, &[("origin", "https://evil.example")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "want 403 for a foreign origin");
    let own: http::Uri = config.url.parse().unwrap_or_else(|e| panic!("url: {e}"));
    let origin = format!("{}://{}", own.scheme_str().unwrap_or(""), own.authority().map_or("", |a| a.as_str()));
    let (status, _) = post(&config.url, &token, LIST_TOOLS, &[("origin", &origin)]).await;
    assert_eq!(status, StatusCode::OK, "the endpoint's own origin must pass");
}

async fn listed(config: &Config) -> Vec<Tool> {
    let client = connect(&config.url, &(config.token)(&[]), ClientOptions::default()).await;
    let first = client.list_tools(None).await.unwrap_or_else(|e| panic!("tools/list: {e}"));
    let again = client.list_tools(None).await.unwrap_or_else(|e| panic!("tools/list: {e}"));
    let names = |tools: &[Tool]| tools.iter().map(|t| t.name.to_string()).collect::<Vec<_>>();
    let (one, two) = (names(&first.tools), names(&again.tools));
    assert!(
        one == two && one.is_sorted() && !one.is_empty(),
        "tools/list must be non-empty, stable and sorted by name, got {one:?} then {two:?}"
    );
    assert_eq!(
        first.cache_scope,
        Some(rmcp::model::CacheScope::Private),
        "tools/list must be cacheable privately only"
    );
    first.tools
}

fn check_tools(tools: &[Tool], prefix: &str) {
    for tool in tools {
        let name = &tool.name;
        assert!(name.starts_with(prefix), "{name}: name must start with {prefix}");
        let described = tool.description.as_deref().is_some_and(|d| !d.is_empty());
        assert!(
            described && tool.title.as_deref().is_some_and(|t| !t.is_empty()),
            "{name}: needs a title and a description"
        );
        let a = tool.annotations.as_ref();
        let complete = a.is_some_and(|a| {
            a.destructive_hint.is_some()
                && a.open_world_hint.is_some()
                && a.title.as_deref().is_some_and(|t| !t.is_empty())
        });
        assert!(complete, "{name}: annotations must set every hint and a title, got {a:?}");
        let read_only = a.and_then(|a| a.read_only_hint) == Some(true);
        assert!(
            !(read_only && a.and_then(|a| a.destructive_hint) == Some(true)),
            "{name}: read-only cannot be destructive"
        );
        check_schema(&format!("{name} input"), &tool.input_schema);
        if let Some(output) = &tool.output_schema {
            check_schema(&format!("{name} output"), output);
        }
    }
}

fn check_schema(label: &str, schema: &JsonObject) {
    assert_eq!(
        schema.get("type"),
        Some(&serde_json::json!("object")),
        "{label}: want an object schema, got {schema:?}"
    );
}

async fn check_write_scope(config: &Config, tools: &[Tool], read: &str, write: &str) {
    let Some(tool) = tools.iter().find(|t| t.annotations.as_ref().and_then(|a| a.read_only_hint) != Some(true)) else {
        return;
    };
    assert!(!write.is_empty(), "{} writes, but the metadata lists no write scope", tool.name);
    let token = (config.token)(&[read]);
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"{}","arguments":{{}}}}}}"#,
        tool.name
    );
    let (status, headers) = post(&config.url, &token, &body, &[]).await;
    let challenge = headers.get(header::WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let stepped =
        challenge.contains("error=\"insufficient_scope\"") && challenge.contains(&format!("scope=\"{write}\""));
    assert!(
        status == StatusCode::FORBIDDEN && stepped,
        "read-only token on {}: want 403 insufficient_scope for {write}, got {status} {challenge:?}",
        tool.name
    );
    let (status, _) = post(&config.url, &token, LIST_TOOLS, &[]).await;
    assert_eq!(status, StatusCode::OK, "a read-only token must list tools");
}
