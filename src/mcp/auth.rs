use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::request::Parts;
use http::{HeaderValue, Method, StatusCode, header};
use serde::Serialize;

use crate::http::plain;

/// The RFC 9728 well-known prefix.
pub const WELL_KNOWN: &str = "/.well-known/oauth-protected-resource";

/// The [`TokenInfo::extra`] key holding the OAuth client the token was issued to, as porte's
/// `mcpauth.ExtraClientID`.
pub const EXTRA_CLIENT_ID: &str = "client_id";

/// The [`TokenInfo::extra`] key holding the agent acting on a delegated token, as porte's
/// `mcpauth.ExtraActor`.
pub const EXTRA_ACTOR: &str = "actor";

/// The [`TokenInfo::extra`] key holding the Registre `sub` of the person the token speaks for,
/// as porte's `mcpauth.ExtraSubject`.
pub const EXTRA_SUBJECT: &str = "subject";

const CLOCK_SKEW: Duration = Duration::from_secs(30);

/// What a [`Verifier`] learned from a valid access token, as go-sdk `auth.TokenInfo`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenInfo {
    /// The app's id of the person the token speaks for.
    pub user_id: String,
    /// The scopes the token grants.
    pub scopes: Vec<String>,
    /// When the token expires. `None` is refused: every access token must expire.
    pub expires_at: Option<SystemTime>,
    /// Claims the kit passes along, keyed by [`EXTRA_CLIENT_ID`], [`EXTRA_ACTOR`], [`EXTRA_SUBJECT`]
    /// and its own.
    pub extra: HashMap<String, String>,
}

/// Why a [`Verifier`] did not accept a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// The token is not valid for this resource. The client gets a 401 and learns nothing more.
    Invalid,
    /// The token could not be checked, such as the issuer's keys being unreachable. A 500.
    Unavailable,
}

/// The future a [`Verifier`] returns.
pub type VerifyFuture<'a> = Pin<Box<dyn Future<Output = Result<TokenInfo, VerifyError>> + Send + 'a>>;

/// Checks MCP access tokens. The app's auth kit implements it (porte's `mcpauth` in the suite);
/// tronc never depends on the kit. It must refuse a token issued for any other resource.
pub trait Verifier: Send + Sync + 'static {
    /// Verifies `token`, sent on the request described by `request`.
    fn verify<'a>(&'a self, token: &'a str, request: &'a Parts) -> VerifyFuture<'a>;
}

/// The RFC 9728 protected resource metadata document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectedResourceMetadata {
    /// The resource URI people paste into their client. Claude refuses a document whose resource
    /// differs from it.
    pub resource: String,
    /// The authorization servers, the issuer alone in the suite.
    pub authorization_servers: Vec<String>,
    /// The scopes, read first. `offline_access` is not a resource scope and is never listed.
    pub scopes_supported: Vec<String>,
    /// Always `["header"]`.
    pub bearer_methods_supported: Vec<String>,
    /// Always `["RS256"]`.
    pub resource_signing_alg_values_supported: Vec<String>,
}

impl ProtectedResourceMetadata {
    /// The document for `resource`, issued by `issuer`, with `scopes`.
    pub fn new(resource: &str, issuer: &str, scopes: Vec<String>) -> Self {
        Self {
            resource: resource.to_owned(),
            authorization_servers: vec![issuer.to_owned()],
            scopes_supported: scopes,
            bearer_methods_supported: vec!["header".to_owned()],
            resource_signing_alg_values_supported: vec!["RS256".to_owned()],
        }
    }
}

/// Where RFC 9728 places the metadata of `resource`: the well-known prefix between the host and
/// the path, so `https://app.example/mcp` maps to
/// `https://app.example/.well-known/oauth-protected-resource/mcp`. `None` when `resource` is not
/// an absolute URI.
pub fn metadata_url(resource: &str) -> Option<String> {
    let uri: http::Uri = resource.parse().ok()?;
    let (scheme, authority) = (uri.scheme_str()?, uri.authority()?);
    Some(format!("{scheme}://{authority}{WELL_KNOWN}{}", uri.path().trim_end_matches('/')))
}

pub(super) struct Gate {
    pub verifier: Arc<dyn Verifier>,
    pub metadata_url: String,
    pub read_scope: String,
}

pub(super) async fn require_token(State(gate): State<Arc<Gate>>, request: Request, next: Next) -> Response {
    let (mut parts, body) = request.into_parts();
    let header = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
    match check(&gate, &header, &parts).await {
        Ok(info) => {
            parts.extensions.insert(info);
            next.run(Request::from_parts(parts, body)).await
        }
        Err((status, message)) => refuse(&gate, status, message, !header.is_empty()),
    }
}

async fn check(gate: &Gate, header: &str, parts: &Parts) -> Result<TokenInfo, (StatusCode, &'static str)> {
    let fields: Vec<&str> = header.split_whitespace().collect();
    let [scheme, token] = fields.as_slice() else {
        return Err((StatusCode::UNAUTHORIZED, "no bearer token\n"));
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err((StatusCode::UNAUTHORIZED, "no bearer token\n"));
    }
    let info = gate.verifier.verify(token, parts).await.map_err(|error| match error {
        VerifyError::Invalid => (StatusCode::UNAUTHORIZED, "invalid token\n"),
        VerifyError::Unavailable => (StatusCode::INTERNAL_SERVER_ERROR, "token verification is unavailable\n"),
    })?;
    if !info.scopes.contains(&gate.read_scope) {
        return Err((StatusCode::FORBIDDEN, "insufficient scope\n"));
    }
    match info.expires_at {
        None => Err((StatusCode::UNAUTHORIZED, "token missing expiration\n")),
        Some(at) if at + CLOCK_SKEW < SystemTime::now() => Err((StatusCode::UNAUTHORIZED, "token expired\n")),
        Some(_) => Ok(info),
    }
}

fn refuse(gate: &Gate, status: StatusCode, message: &'static str, with_token: bool) -> Response {
    let mut response = plain(status, message);
    let code = match status {
        StatusCode::FORBIDDEN => "error=\"insufficient_scope\", ",
        StatusCode::UNAUTHORIZED if with_token => "error=\"invalid_token\", ",
        StatusCode::UNAUTHORIZED => "",
        _ => return response,
    };
    let challenge = format!("Bearer {code}resource_metadata=\"{}\", scope=\"{}\"", gate.metadata_url, gate.read_scope);
    if let Ok(value) = HeaderValue::from_str(&challenge) {
        response.headers_mut().insert(header::WWW_AUTHENTICATE, value);
    }
    response
}

pub(super) async fn serve_metadata(
    State(document): State<Arc<ProtectedResourceMetadata>>,
    request: Request,
) -> Response {
    let mut response = match *request.method() {
        Method::OPTIONS => plain(StatusCode::NO_CONTENT, ""),
        Method::GET => crate::json::write(StatusCode::OK, document.as_ref()),
        _ => plain(StatusCode::METHOD_NOT_ALLOWED, "Method not allowed\n"),
    };
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, OPTIONS"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Content-Type"));
    response
}
