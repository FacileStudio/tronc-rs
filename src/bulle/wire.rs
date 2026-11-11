use reqwest::StatusCode;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::error::{Error, PlanRequired};
use crate::errors::safe_upgrade_url;

pub(super) fn unreachable(error: reqwest::Error) -> Error {
    Error::Unreachable(error.without_url().to_string())
}

pub(super) fn answer(status: StatusCode, raw: Vec<u8>) -> Result<Vec<u8>, Error> {
    match status.as_u16() {
        500.. => Err(Error::Unreachable(format!("answered {}", status.as_u16()))),
        402 => Err(plan_required(&raw)),
        400..=499 => {
            let fields = envelope(&raw);
            Err(Error::Status { status: status.as_u16(), code: fields.code, message: fields.message })
        }
        200..=299 => Ok(raw),
        other => Err(Error::Unreachable(format!("answered {other}"))),
    }
}

pub(super) fn decode<T: DeserializeOwned>(raw: &[u8]) -> Result<T, Error> {
    let body = if raw.trim_ascii().is_empty() { b"null".as_slice() } else { raw };
    serde_json::from_slice(body).map_err(|e| Error::Unreachable(format!("unreadable answer: {e}")))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Fields {
    code: String,
    message: String,
    upgrade_url: String,
    key: String,
}

/// Reads an error body in tronc's `{"error":{...}}` envelope, as `{"error":"message"}`, or bare
/// with the fields at the top level. Anything else gives no fields.
fn envelope(raw: &[u8]) -> Fields {
    let Ok(Value::Object(body)) = serde_json::from_slice::<Value>(raw) else {
        return Fields::default();
    };
    match body.get("error") {
        Some(inner @ Value::Object(_)) => Fields::deserialize(inner).unwrap_or_default(),
        Some(Value::String(message)) => Fields { message: message.clone(), ..Fields::default() },
        _ => Fields::deserialize(Value::Object(body)).unwrap_or_default(),
    }
}

fn plan_required(raw: &[u8]) -> Error {
    let fields = envelope(raw);
    let upgrade_url = if safe_upgrade_url(&fields.upgrade_url) { fields.upgrade_url } else { String::new() };
    Error::PlanRequired(PlanRequired { message: fields.message, upgrade_url, key: fields.key })
}
