use std::time::{Duration, SystemTime};

use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::error::{Error, PlanRequired};
use crate::errors::safe_upgrade_url;

pub(super) fn unreachable(error: reqwest::Error) -> Error {
    Error::Unreachable(error.without_url().to_string())
}

const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 3600);

pub(super) fn answer(status: StatusCode, wait: Duration, raw: Vec<u8>) -> Result<Vec<u8>, Error> {
    match status.as_u16() {
        503 if !wait.is_zero() => Err(refused(503, wait, &raw)),
        500.. => Err(Error::Unreachable(format!("answered {}", status.as_u16()))),
        402 => Err(plan_required(&raw)),
        429 => Err(refused(429, wait, &raw)),
        400..=499 => Err(refused(status.as_u16(), Duration::ZERO, &raw)),
        200..=299 => Ok(raw),
        other => Err(Error::Unreachable(format!("answered {other}"))),
    }
}

/// Reads a `Retry-After` header, delay seconds or an HTTP date, as the wait from `now`, at most
/// a day. A missing, unreadable or past value is zero.
pub(super) fn retry_after(headers: &HeaderMap, now: SystemTime) -> Duration {
    let value = headers.get(RETRY_AFTER).and_then(|v| v.to_str().ok()).unwrap_or_default().trim();
    if let Ok(seconds) = value.parse::<i64>() {
        return Duration::from_secs(seconds.clamp(0, MAX_RETRY_AFTER.as_secs() as i64) as u64);
    }
    let Ok(when) = httpdate::parse_http_date(value) else {
        return Duration::ZERO;
    };
    let left = when.duration_since(now).unwrap_or_default();
    Duration::from_secs(((left.as_millis() + 500) / 1000) as u64).min(MAX_RETRY_AFTER)
}

fn refused(status: u16, retry_after: Duration, raw: &[u8]) -> Error {
    let fields = envelope(raw);
    Error::Status { status, code: fields.code, message: fields.message, retry_after }
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
    limit: i64,
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
    Error::PlanRequired(PlanRequired { message: fields.message, upgrade_url, key: fields.key, limit: fields.limit })
}
