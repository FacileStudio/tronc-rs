use std::error::Error as StdError;

use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};

use super::{Code, Error, safe_upgrade_url, status_for};
use crate::json;

/// The envelope body for any wire code, `{"error":{"code","message"}}`, plus `upgrade_url`
/// when it is safe.
pub fn envelope(code: &str, message: &str, upgrade_url: Option<&str>) -> Value {
    let mut body = Map::new();
    body.insert("code".into(), Value::from(code));
    body.insert("message".into(), Value::from(message));
    if let Some(url) = upgrade_url.filter(|url| safe_upgrade_url(url)) {
        body.insert("upgrade_url".into(), Value::from(url));
    }
    json!({ "error": body })
}

/// Writes an envelope for any wire code, with the status [`status_for`] maps it to. Unknown and
/// empty codes answer 500, as in Go.
pub fn envelope_response(code: &str, message: &str, upgrade_url: Option<&str>) -> Response {
    json::write(status_for(code), &envelope(code, message, upgrade_url))
}

/// Writes any error, as Go `httpjson.WriteError`: the first [`Error`] in the source chain is
/// answered as itself, and anything else becomes a generic internal error so a raw failure
/// never leaks its text.
pub fn write_error(error: &(dyn StdError + 'static)) -> Response {
    let mut current = Some(error);
    while let Some(candidate) = current {
        if let Some(found) = candidate.downcast_ref::<Error>() {
            return found.respond();
        }
        current = candidate.source();
    }
    tracing::error!(error = %error, "internal error");
    envelope_response(Code::Internal.as_str(), "internal server error", None)
}

impl Error {
    fn respond(&self) -> Response {
        if self.code == Code::Internal {
            match &self.cause {
                Some(cause) => tracing::error!(detail = %self.message, error = %cause, "internal error"),
                None => tracing::error!(detail = %self.message, "internal error"),
            }
        }
        envelope_response(self.code.as_str(), &self.message, self.upgrade_url.as_deref())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        self.respond()
    }
}
