//! The suite error envelope: a stable code, a human message, and a cause kept for the logs.

use std::error::Error as StdError;

use http::StatusCode;

/// The stable, machine-readable code every Facile API puts in its error envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    /// A malformed or unacceptable request. HTTP 400.
    InvalidArgument,
    /// A missing or unusable credential. HTTP 401.
    Unauthenticated,
    /// A feature or quota the caller's plan does not include. HTTP 402.
    PaymentRequired,
    /// A valid credential that lacks the right. HTTP 403.
    PermissionDenied,
    /// The addressed resource does not exist. HTTP 404.
    NotFound,
    /// The resource already exists or has moved on. HTTP 409.
    AlreadyExists,
    /// The system state rules the operation out. HTTP 412.
    FailedPrecondition,
    /// A payload the server refuses to accept. HTTP 413.
    ResourceExhausted,
    /// The caller has spent its quota. HTTP 429.
    RateLimited,
    /// A verb the route does not serve. HTTP 405.
    MethodNotAllowed,
    /// A dependency that is down or still starting. HTTP 503.
    Unavailable,
    /// A server-side fault. HTTP 500.
    Internal,
}

const CODES: [(Code, &str, u16); 12] = [
    (Code::InvalidArgument, "invalid_argument", 400),
    (Code::Unauthenticated, "unauthenticated", 401),
    (Code::PaymentRequired, "payment_required", 402),
    (Code::PermissionDenied, "permission_denied", 403),
    (Code::NotFound, "not_found", 404),
    (Code::AlreadyExists, "already_exists", 409),
    (Code::FailedPrecondition, "failed_precondition", 412),
    (Code::ResourceExhausted, "resource_exhausted", 413),
    (Code::RateLimited, "rate_limited", 429),
    (Code::MethodNotAllowed, "method_not_allowed", 405),
    (Code::Unavailable, "unavailable", 503),
    (Code::Internal, "internal", 500),
];

impl Code {
    /// The wire spelling of the code, as in `"not_found"`.
    pub const fn as_str(self) -> &'static str {
        CODES[self as usize].1
    }

    /// The HTTP status the code maps to.
    pub const fn status(self) -> StatusCode {
        match StatusCode::from_u16(CODES[self as usize].2) {
            Ok(status) => status,
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Reads a wire spelling back into a code, or `None` for a code the suite does not define.
    pub fn parse(value: &str) -> Option<Self> {
        CODES.iter().find(|(_, name, _)| *name == value).map(|(code, _, _)| *code)
    }
}

/// The status for a wire code, with anything unknown mapped to 500, as Go `errors.Status` does.
pub fn status_for(code: &str) -> StatusCode {
    Code::parse(code).map_or(StatusCode::INTERNAL_SERVER_ERROR, Code::status)
}

/// The cause an [`Error`] keeps for the logs and never sends to a client.
pub type Cause = Box<dyn StdError + Send + Sync>;

/// The envelope every Facile API returns for a failure.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct Error {
    /// The stable identifier clients branch on.
    pub code: Code,
    /// The human-readable message, sent to the client.
    pub message: String,
    /// Where the caller can upgrade, sent only when [`safe_upgrade_url`] accepts it.
    pub upgrade_url: Option<String>,
    /// The entitlement a plan refusal names, such as `ai.access`, sent as `key` when set. muse's
    /// `planLimitFrom` reads it.
    pub key: Option<String>,
    /// The underlying failure, kept out of the payload.
    #[source]
    pub cause: Option<Cause>,
}

/// Reports whether a URL may be sent as `upgrade_url`: a same-origin path, or an `https` URL
/// with a host and no credentials.
pub fn safe_upgrade_url(raw: &str) -> bool {
    if raw.is_empty() || raw.chars().any(|c| c.is_control() || c == ' ' || c == '\\') {
        return false;
    }
    if let Some(rest) = raw.strip_prefix('/') {
        return !rest.starts_with('/');
    }
    match raw.parse::<http::Uri>() {
        Ok(uri) => {
            let authority = uri.authority().map(|a| a.as_str()).unwrap_or("");
            uri.scheme_str() == Some("https") && uri.host().is_some_and(|h| !h.is_empty()) && !authority.contains('@')
        }
        Err(_) => false,
    }
}

mod ctor;
#[cfg(feature = "http")]
mod response;

#[cfg(feature = "http")]
pub use response::{envelope, envelope_response, write_error};

/// A `Result` whose error is the suite envelope.
pub type Result<T> = std::result::Result<T, Error>;
