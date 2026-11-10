use http::request::Parts;
use rmcp::model::{ClientCapabilities, InputResponses, ProtocolVersion};

use super::auth::{EXTRA_ACTOR, EXTRA_CLIENT_ID, TokenInfo};

/// The first protocol version with multi round-trip requests. Older clients reach a stateless
/// server without a session, so a server-to-client elicitation cannot run for them.
pub const MRTR_VERSION: &str = "2026-07-28";

/// The person a tool call acts for, and the client acting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caller {
    /// The app's id of the person, as the verifier resolved it.
    pub user_id: String,
    /// The OAuth client the token was issued to.
    pub client_id: String,
    /// The agent acting for the person on a delegated token, empty otherwise.
    pub actor: String,
    /// The scopes the token grants.
    pub scopes: Vec<String>,
}

impl Caller {
    /// Reads the caller from verified token info. `None` when it names nobody.
    pub fn from_token(info: &TokenInfo) -> Option<Self> {
        if info.user_id.is_empty() {
            return None;
        }
        let extra = |key: &str| info.extra.get(key).cloned().unwrap_or_default();
        Some(Self {
            user_id: info.user_id.clone(),
            client_id: extra(EXTRA_CLIENT_ID),
            actor: extra(EXTRA_ACTOR),
            scopes: info.scopes.clone(),
        })
    }

    /// Reports whether the caller's token grants `scope`.
    pub fn has(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
}

/// One tool call as its handler sees it: who calls, the HTTP request, and the MRTR fields of a
/// retry.
#[derive(Debug, Clone)]
pub struct Call {
    /// The person and client behind the verified token.
    pub caller: Caller,
    /// The HTTP request head, with the app's own extensions.
    pub parts: Parts,
    /// The protocol version the request speaks.
    pub protocol_version: Option<ProtocolVersion>,
    /// The capabilities the client declared on this request.
    pub capabilities: Option<ClientCapabilities>,
    /// The `requestState` a retry echoes back.
    pub request_state: Option<String>,
    /// The answers a retry carries.
    pub input_responses: InputResponses,
}

impl Call {
    /// Reports whether the client asked for form elicitation on a protocol version that lets a
    /// stateless server use it. An elicitation capability naming no mode means form.
    pub fn can_elicit(&self) -> bool {
        if self.protocol_version.as_ref().is_none_or(|v| v.as_str() < MRTR_VERSION) {
            return false;
        }
        let elicitation = self.capabilities.as_ref().and_then(|c| c.elicitation.as_ref());
        elicitation.is_some_and(|e| e.form.is_some() || e.url.is_none())
    }

    /// Reports whether the call is the retry of an `input_required` result. A sensitive tool
    /// checks it first and hands the call to [`super::Confirmations::resume`].
    pub fn retrying(&self) -> bool {
        self.request_state.as_deref().is_some_and(|s| !s.is_empty()) || !self.input_responses.is_empty()
    }
}
