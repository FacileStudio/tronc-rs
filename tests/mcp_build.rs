#![cfg(feature = "mcp")]

use std::sync::Arc;

use http::request::Parts;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tronc::mcp::{self, Annotations, Answer, Call, Options, Scopes, Server, TokenInfo, Tool, ToolError, ToolSpec};
use tronc::mcp::{Verifier, VerifyFuture};

struct AcceptAll;

impl Verifier for AcceptAll {
    fn verify<'a>(&'a self, _token: &'a str, _request: &'a Parts) -> VerifyFuture<'a> {
        Box::pin(async { Ok(TokenInfo { user_id: "1".into(), ..TokenInfo::default() }) })
    }
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct Nothing {}

fn noop(name: &str, title: &str, annotations: Option<Annotations>) -> Tool {
    let spec = ToolSpec { name: name.into(), title: title.into(), description: "D".into(), annotations };
    mcp::tool(spec, |_call: Call, _in: Nothing| async { Ok::<_, ToolError>(Answer::Output(Nothing {})) })
}

fn options(tools: Vec<Tool>) -> Options {
    let mut options = Options::new("app", "https://app.example/mcp", "https://i.example", Arc::new(AcceptAll));
    options.tools = tools;
    options
}

type Change = (&'static str, fn(&mut Options));

const READ: Option<Annotations> =
    Some(Annotations { read_only: true, destructive: false, idempotent: false, open_world: false });

#[test]
fn bad_tools_are_refused() {
    let both = Some(Annotations { read_only: true, destructive: true, ..Annotations::default() });
    let cases = [
        ("no prefix", noop("list", "T", READ)),
        ("other prefix", noop("other_list", "T", READ)),
        ("upper case", noop("app_List", "T", READ)),
        ("bare prefix", noop("app_", "T", READ)),
        ("no annotations", noop("app_list", "T", None)),
        ("no title", noop("app_list", "", READ)),
        ("read-only and destructive", noop("app_list", "T", both)),
    ];
    for (name, tool) in cases {
        assert!(Server::new(options(vec![tool])).is_err(), "{name}: want an error");
    }
    let twice = noop("app_list", "T", READ);
    assert!(Server::new(options(vec![twice.clone(), twice])).is_err(), "a duplicate tool must be refused");
}

#[test]
fn bad_options_are_refused_and_defaults_filled() {
    let cases: [Change; 4] = [
        ("relative resource", |o| o.resource = "/mcp".into()),
        ("no issuer", |o| o.issuer.clear()),
        ("bad name", |o| o.name = "My App".into()),
        ("no read scope", |o| o.scopes = Scopes { read: String::new(), write: "app.write".into() }),
    ];
    for (name, change) in cases {
        let mut opts = options(Vec::new());
        change(&mut opts);
        assert!(Server::new(opts).is_err(), "{name}: want an error");
    }
    let server = Server::new(options(Vec::new())).expect("defaults");
    assert_eq!(server.metadata_url(), "https://app.example/.well-known/oauth-protected-resource/mcp");
}

#[test]
fn a_read_only_server_takes_only_read_tools() {
    let mut opts = options(vec![noop("app_list", "T", READ)]);
    opts.scopes = Scopes { read: "app.read".into(), write: String::new() };
    assert_eq!(opts.scopes.list(), ["app.read"]);
    assert!(Server::new(opts.clone()).is_ok(), "a read tool on a read-only server is fine");
    opts.tools = vec![noop("app_write", "T", Some(Annotations::default()))];
    assert!(Server::new(opts).is_err(), "a write tool on a read-only server must be refused");
}

#[test]
fn metadata_urls_follow_rfc_9728() {
    assert_eq!(
        mcp::metadata_url("https://app.example/mcp/").as_deref(),
        Some("https://app.example/.well-known/oauth-protected-resource/mcp")
    );
    assert_eq!(
        mcp::metadata_url("https://app.example").as_deref(),
        Some("https://app.example/.well-known/oauth-protected-resource")
    );
    assert_eq!(mcp::metadata_url("/mcp"), None);
}
