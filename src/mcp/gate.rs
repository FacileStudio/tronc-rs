use std::collections::HashSet;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::{HeaderValue, Method, StatusCode, header};
use serde::Deserialize;

use super::auth::TokenInfo;
use crate::http::plain;

/// The largest request body the endpoint reads, rmcp's own default: 4 MiB.
pub const MAX_REQUEST_BODY_BYTES: usize = 4 << 20;

pub(super) struct WriteGate {
    pub writes: HashSet<String>,
    pub scope: String,
    pub challenge: HeaderValue,
    pub refusal: String,
}

impl WriteGate {
    pub fn new(writes: HashSet<String>, scope: &str, metadata_url: &str) -> Self {
        let challenge =
            format!("Bearer error=\"insufficient_scope\", scope=\"{scope}\", resource_metadata=\"{metadata_url}\"");
        let refusal = format!("Forbidden: this tool needs the {scope} scope\n");
        Self {
            writes,
            scope: scope.to_owned(),
            challenge: HeaderValue::from_str(&challenge).unwrap_or_else(|_| HeaderValue::from_static("Bearer")),
            refusal,
        }
    }
}

#[derive(Deserialize)]
struct RpcCall {
    #[serde(default)]
    method: String,
    #[serde(default)]
    params: Option<RpcParams>,
}

#[derive(Deserialize)]
struct RpcParams {
    #[serde(default)]
    name: String,
}

/// Reports whether a JSON-RPC message, or any message of a batch, calls one of `names`.
pub(super) fn calls_any(body: &[u8], names: &HashSet<String>) -> bool {
    let calls = serde_json::from_slice::<Vec<RpcCall>>(body)
        .or_else(|_| serde_json::from_slice::<RpcCall>(body).map(|call| vec![call]))
        .unwrap_or_default();
    calls.iter().any(|call| {
        call.method == "tools/call" && call.params.as_ref().is_some_and(|params| names.contains(&params.name))
    })
}

pub(super) async fn write_gate(State(gate): State<Arc<WriteGate>>, request: Request, next: Next) -> Response {
    let granted = request.extensions().get::<TokenInfo>().is_some_and(|info| info.scopes.contains(&gate.scope));
    if request.method() != Method::POST || granted {
        return next.run(request).await;
    }
    let (parts, body) = request.into_parts();
    let Ok(body) = to_bytes(body, MAX_REQUEST_BODY_BYTES).await else {
        return plain(StatusCode::PAYLOAD_TOO_LARGE, "request body too large\n");
    };
    if calls_any(&body, &gate.writes) {
        let mut response = plain(StatusCode::FORBIDDEN, gate.refusal.clone());
        response.headers_mut().insert(header::WWW_AUTHENTICATE, gate.challenge.clone());
        return response;
    }
    next.run(Request::from_parts(parts, Body::from(body))).await
}

pub(super) async fn origin_guard(State(allowed): State<Arc<Vec<String>>>, request: Request, next: Next) -> Response {
    let origin = request.headers().get(header::ORIGIN).map(|v| v.to_str().unwrap_or("\u{0}"));
    if origin.is_some_and(|origin| !origin.is_empty() && !allowed.iter().any(|a| a == origin)) {
        return plain(StatusCode::FORBIDDEN, "Forbidden: origin not allowed\n");
    }
    next.run(request).await
}

/// Answers a GET or DELETE on the stateless endpoint with a plain-text 405, as Go's endpoint
/// does: there is no session stream to open or close.
pub(super) async fn post_only(request: Request, next: Next) -> Response {
    if !matches!(*request.method(), Method::GET | Method::DELETE) {
        return next.run(request).await;
    }
    let mut response = plain(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed\n");
    response.headers_mut().insert(header::ALLOW, HeaderValue::from_static("POST"));
    response
}
