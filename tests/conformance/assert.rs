use http::HeaderMap;
use serde_json::Value;

use super::support::{Answer, string_map};

pub fn expect_eq<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Result<(), String> {
    if got == want { Ok(()) } else { Err(format!("{what}: got {got:?}, want {want:?}")) }
}

pub fn expect_answer(answer: &Answer, want: &Value) -> Result<(), String> {
    if let Some(status) = want.get("status").and_then(Value::as_u64) {
        expect_eq("status", u64::from(answer.status.as_u16()), status)?;
    }
    if let Some(body) = want.get("body").and_then(Value::as_str) {
        expect_eq("body", String::from_utf8_lossy(&answer.body).as_ref(), body)?;
    }
    if let Some(content_type) = want.get("content_type").and_then(Value::as_str) {
        expect_eq("content type", header(&answer.headers, "content-type"), content_type)?;
    }
    for (name, value) in string_map(want.get("headers")) {
        expect_eq(&name, header(&answer.headers, &name), value.as_str())?;
    }
    Ok(())
}

pub fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("")
}
