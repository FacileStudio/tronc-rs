use std::sync::Arc;

use http::{HeaderMap, StatusCode};
use rmcp::model::{
    ClientCapabilities, ClientConfig, ElicitRequestParams, ElicitResult, ElicitationAction, Implementation,
    ProtocolVersion,
};
use rmcp::service::{RequestContext, RoleClient, RunningService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{ClientHandler, ClientLifecycleMode, ClientServiceExt, ErrorData};

/// Answers one elicitation.
pub type Elicit = Arc<dyn Fn() -> ElicitResult + Send + Sync>;

/// How a test client behaves.
#[derive(Clone, Default)]
pub struct ClientOptions {
    /// Answers confirmation questions. `None` means the client does not advertise elicitation.
    pub elicit: Option<Elicit>,
    /// The protocol version to ask for. `None` means 2026-07-28, negotiated per request through
    /// `server/discover`; an older version runs the legacy `initialize` handshake.
    pub version: Option<ProtocolVersion>,
}

/// The rmcp client handler behind [`connect`].
#[derive(Clone)]
pub struct TestClient {
    options: ClientOptions,
}

impl ClientHandler for TestClient {
    fn get_info(&self) -> ClientConfig {
        let capabilities = if self.options.elicit.is_some() {
            ClientCapabilities::builder().enable_elicitation().build()
        } else {
            ClientCapabilities::default()
        };
        let info = ClientConfig::new(capabilities, Implementation::new("mcptest", "1"));
        info.with_protocol_version(self.options.version.clone().unwrap_or(ProtocolVersion::V_2026_07_28))
    }

    async fn create_elicitation(
        &self,
        _request: ElicitRequestParams,
        _context: RequestContext<RoleClient>,
    ) -> Result<ElicitResult, ErrorData> {
        Ok(self
            .options
            .elicit
            .as_ref()
            .map_or_else(|| ElicitResult::new(ElicitationAction::Decline), |answer| answer()))
    }
}

/// A connected test client.
pub type Client = RunningService<RoleClient, TestClient>;

/// Opens an rmcp client on `endpoint` with bearer `token`.
pub async fn connect(endpoint: &str, token: &str, options: ClientOptions) -> Client {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(endpoint.to_owned()).auth_header(token.to_owned()),
    );
    let lifecycle = match &options.version {
        None => ClientLifecycleMode::Discover { preferred_versions: vec![ProtocolVersion::V_2026_07_28] },
        Some(_) => ClientLifecycleMode::Initialize,
    };
    let client = TestClient { options };
    client.serve_with_lifecycle(transport, lifecycle).await.unwrap_or_else(|e| panic!("mcptest: connect: {e}"))
}

/// Sends one raw JSON-RPC body and returns the status and headers.
pub async fn post(endpoint: &str, token: &str, body: &str, headers: &[(&str, &str)]) -> (StatusCode, HeaderMap) {
    let mut request = reqwest::Client::new()
        .post(endpoint)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(body.to_owned());
    if !token.is_empty() {
        request = request.bearer_auth(token);
    }
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = request.send().await.unwrap_or_else(|e| panic!("mcptest: post {endpoint}: {e}"));
    (response.status(), response.headers().clone())
}

/// The RFC 9728 location of `resource`'s metadata.
pub fn metadata_url_of(resource: &str) -> String {
    crate::mcp::metadata_url(resource).unwrap_or_else(|| panic!("mcptest: {resource} is not an absolute URI"))
}
