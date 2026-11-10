use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::Router;
use http::request::Parts;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tronc::mcp::{self, Annotations, Answer, Call, EXTRA_CLIENT_ID, Options, TokenInfo, ToolError, ToolSpec};
use tronc::mcp::{Verifier, VerifyError, VerifyFuture};

struct DevTokens;

impl Verifier for DevTokens {
    fn verify<'a>(&'a self, token: &'a str, _request: &'a Parts) -> VerifyFuture<'a> {
        Box::pin(async move {
            let (user, scopes) = token.split_once('|').ok_or(VerifyError::Invalid)?;
            Ok(TokenInfo {
                user_id: user.to_owned(),
                scopes: scopes.split(',').map(str::to_owned).collect(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(600)),
                extra: HashMap::from([(EXTRA_CLIENT_ID.to_owned(), "dev-client".to_owned())]),
            })
        })
    }
}

#[derive(Deserialize, JsonSchema)]
struct Name {
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct Greeting {
    text: String,
}

async fn greet(call: Call, input: Name) -> Result<Answer<Greeting>, ToolError> {
    Ok(Greeting { text: format!("hello {}, from {}", input.name, call.caller.user_id) }.into())
}

fn spec(name: &str, title: &str, annotations: Annotations) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        title: title.into(),
        description: format!("{title}."),
        annotations: Some(annotations),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_owned());
    let resource = format!("http://127.0.0.1:{port}/mcp");
    let mut options = Options::new("hello", resource, "https://sso.facile.studio", Arc::new(DevTokens));
    options.tools = vec![
        mcp::tool(
            spec("hello_greet", "Greet someone", Annotations { read_only: true, ..Annotations::default() }),
            greet,
        ),
        mcp::tool(spec("hello_shout", "Shout at someone", Annotations::default()), greet),
    ];
    let app = mcp::mount(Router::new(), options)?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port.parse::<u16>()?)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
