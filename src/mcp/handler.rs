use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use http::request::Parts;
use rmcp::model::{
    CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerConfig,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData, ServerHandler};

use super::audit::{Event, audit};
use super::auth::TokenInfo;
use super::call::{Call, Caller};
use super::failure::ToolError;
use super::tool::Tool;

const NO_TOKEN: &str = "this request carries no valid access token. Reconnect the app";

pub(super) struct Toolset {
    pub tools: BTreeMap<String, Tool>,
    pub listed: Vec<rmcp::model::Tool>,
    pub info: ServerConfig,
    pub write_scope: String,
    pub list_ttl: Duration,
}

#[derive(Clone)]
pub(super) struct Handler(pub Arc<Toolset>);

fn refusal(message: impl Into<String>) -> CallToolResponse {
    CallToolResponse::Complete(CallToolResult::error(vec![ContentBlock::text(message.into())]))
}

fn outcome(result: &Result<CallToolResponse, ToolError>) -> &'static str {
    match result {
        Ok(CallToolResponse::Complete(done)) if done.is_error != Some(true) => "ok",
        Ok(CallToolResponse::InputRequired(_)) => "input_required",
        _ => "error",
    }
}

fn caller(parts: Option<&Parts>) -> Option<Caller> {
    parts?.extensions.get::<TokenInfo>().and_then(Caller::from_token)
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        self.0.info.clone()
    }

    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.0.tools.get(name).map(Tool::definition)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let ttl = u64::try_from(self.0.list_ttl.as_millis()).unwrap_or(u64::MAX);
        Ok(ListToolsResult::with_all_items(self.0.listed.clone())
            .with_ttl_ms(ttl)
            .with_cache_scope(CacheScope::Private))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(tool) = self.0.tools.get(request.name.as_ref()) else {
            return Err(ErrorData::invalid_params(format!("unknown tool {:?}", request.name), None));
        };
        let parts = context.extensions.get::<Parts>();
        let (Some(caller), Some(parts)) = (caller(parts), parts.cloned()) else {
            return Ok(refusal(NO_TOKEN));
        };
        let read_only = tool.spec.annotations.is_some_and(|a| a.read_only);
        let scope = &self.0.write_scope;
        if !read_only && !caller.has(scope) {
            return Ok(refusal(format!("this tool needs the {scope} scope. Reconnect the app and grant write access")));
        }
        let call = Call {
            caller: caller.clone(),
            parts,
            protocol_version: context.protocol_version(),
            capabilities: context.meta.client_capabilities(),
            request_state: request.request_state,
            input_responses: request.input_responses.unwrap_or_default(),
        };
        let result = (tool.handler)(call, request.arguments.unwrap_or_default()).await;
        if !read_only {
            audit(Event { tool: &tool.spec.name, outcome: outcome(&result), caller: &caller, target: "" });
        }
        Ok(result.unwrap_or_else(|error| refusal(error.0)))
    }
}
