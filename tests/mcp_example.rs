#![cfg(feature = "mcptest")]

use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tronc::mcptest::{ClientOptions, call, connect, decode};

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Deserialize)]
struct Greeting {
    text: String,
}

async fn boot() -> (Server, String) {
    let port = TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port();
    let binary = std::env::current_exe().expect("test binary");
    let binary = binary.parent().and_then(|d| d.parent()).expect("target dir").join("examples").join("mcp");
    let server = Server(Command::new(binary).env("PORT", port.to_string()).spawn().expect("spawn the example"));
    let metadata = format!("http://127.0.0.1:{port}/.well-known/oauth-protected-resource/mcp");
    let deadline = Instant::now() + Duration::from_secs(10);
    while reqwest::get(&metadata).await.is_err() {
        assert!(Instant::now() < deadline, "the example never answered");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    (server, format!("http://127.0.0.1:{port}/mcp"))
}

fn args(name: &str) -> rmcp::model::JsonObject {
    serde_json::from_value(serde_json::json!({ "name": name })).expect("arguments")
}

#[tokio::test]
async fn an_rmcp_client_lists_and_calls_the_example_and_a_read_token_cannot_write() {
    let (_server, url) = boot().await;
    let writer = connect(&url, "42|hello.read,hello.write", ClientOptions::default()).await;
    let names: Vec<String> = writer.list_all_tools().await.expect("list").iter().map(|t| t.name.to_string()).collect();
    assert_eq!(names, ["hello_greet", "hello_shout"]);
    let greeting: Greeting = decode(&call(&writer, "hello_greet", args("Ada")).await);
    assert_eq!(greeting.text, "hello Ada, from 42");
    let shout: Greeting = decode(&call(&writer, "hello_shout", args("Ada")).await);
    assert_eq!(shout.text, "hello Ada, from 42");

    let reader = connect(&url, "42|hello.read", ClientOptions::default()).await;
    let read: Greeting = decode(&call(&reader, "hello_greet", args("Bob")).await);
    assert_eq!(read.text, "hello Bob, from 42");
    let params = rmcp::model::CallToolRequestParams::new("hello_shout").with_arguments(args("Bob"));
    let refused = reader.call_tool(params).await.expect_err("a read-only token must not write");
    assert!(refused.to_string().to_lowercase().contains("insufficient scope"), "{refused}");
}
