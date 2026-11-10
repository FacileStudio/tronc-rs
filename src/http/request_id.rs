use std::convert::Infallible;

use axum::extract::{FromRequestParts, Request};
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderValue;
use http::request::Parts;

/// The header a request id travels in, both ways.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// The longest request id accepted off the wire.
pub const MAX_REQUEST_ID_LENGTH: usize = 128;

/// The id the chassis attached to a request: the caller's when well formed, or a fresh one.
/// It is a hint for correlating logs, never a credential.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestId(pub String);

impl<S: Send + Sync> FromRequestParts<S> for RequestId {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<RequestId>().cloned().unwrap_or_default())
    }
}

/// Returns the caller's id when it is at most [`MAX_REQUEST_ID_LENGTH`] bytes of
/// alphanumerics and `-_.:/`, or `None` to mint a new one.
pub fn accept(value: &str) -> Option<&str> {
    let allowed = |b: u8| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b);
    (!value.is_empty() && value.len() <= MAX_REQUEST_ID_LENGTH && value.bytes().all(allowed)).then_some(value)
}

/// Mints an opaque id: 26 characters of the RFC 4648 base32 alphabet, as Go `rand.Text`.
pub fn mint() -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bytes = [0_u8; 26];
    getrandom::fill(&mut bytes).expect("the operating system random source failed");
    bytes.iter().map(|b| ALPHABET[usize::from(b % 32)] as char).collect()
}

pub async fn attach(mut request: Request, next: Next) -> Response {
    let inbound = request.headers().get(REQUEST_ID_HEADER).and_then(|v| v.to_str().ok()).and_then(accept);
    let id = inbound.map_or_else(mint, str::to_owned);
    request.extensions_mut().insert(RequestId(id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}
