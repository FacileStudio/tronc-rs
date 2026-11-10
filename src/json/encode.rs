use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header};
use serde::Serialize;

/// Encodes a value the way Go's `json.Encoder` does: compact, with `<`, `>`, `&`, U+2028 and
/// U+2029 escaped, and a trailing newline. Responses therefore match Go tronc byte for byte.
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned());
    let mut out = String::with_capacity(raw.len() + 1);
    for c in raw.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            other => out.push(other),
        }
    }
    out.push('\n');
    out.into_bytes()
}

/// Writes a value as `application/json` with the given status, as Go `httpjson.WriteJSON` does.
pub fn write<T: Serialize + ?Sized>(status: StatusCode, value: &T) -> Response {
    let mut response = (status, encode(value)).into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    response
}
