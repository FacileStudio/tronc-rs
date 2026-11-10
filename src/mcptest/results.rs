use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ElicitResult, ElicitationAction, JsonObject,
};
use serde::de::DeserializeOwned;

use super::client::{Client, Elicit};

/// Runs tool `name`, letting the client answer `input_required` rounds, and panics on a protocol
/// error.
pub async fn call(client: &Client, name: &str, args: JsonObject) -> CallToolResult {
    let params = CallToolRequestParams::new(name.to_owned()).with_arguments(args);
    client.call_tool(params).await.unwrap_or_else(|e| panic!("mcptest: {name}: {e}"))
}

/// Sends one `tools/call` and returns the result or the `input_required` question as is.
pub async fn call_once(client: &Client, params: CallToolRequestParams) -> CallToolResponse {
    let name = params.name.clone();
    client.call_tool_once(params).await.unwrap_or_else(|e| panic!("mcptest: {name}: {e}"))
}

/// Reads a result's structured content into `T`, panicking on an error result.
pub fn decode<T: DeserializeOwned>(result: &CallToolResult) -> T {
    assert!(result.is_error != Some(true), "mcptest: tool failed: {}", text(result));
    let value = result.structured_content.clone().unwrap_or_default();
    serde_json::from_value(value).unwrap_or_else(|e| panic!("mcptest: structured content: {e}"))
}

/// Joins a result's text content.
pub fn text(result: &CallToolResult) -> String {
    result.content.iter().filter_map(|c| c.as_text().map(|t| t.text.clone())).collect::<Vec<_>>().join("\n")
}

/// An elicitation answer of `action`, ticking the confirm box on an accept, that counts the
/// questions in `asked`.
pub fn answering(action: ElicitationAction, asked: Arc<AtomicUsize>) -> Elicit {
    Arc::new(move || {
        asked.fetch_add(1, Ordering::SeqCst);
        let accept = action == ElicitationAction::Accept;
        ElicitResult::new(action.clone()).with_content(serde_json::json!({ "confirm": accept }))
    })
}
