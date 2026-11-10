use http::{HeaderMap, HeaderName, HeaderValue, Uri};
use serde_json::{Value, json};
use tronc::http::{classify, level_for, log_client_ip, redact_query};

use super::assert::expect_eq;
use super::support::{Case, string_map, text};

pub fn check(case: &Case) -> Result<(), String> {
    let input = &case.input;
    let uri: Uri = text(input, "url").parse().map_err(|e| format!("url: {e}"))?;
    let prefix = input.get("api_prefix").and_then(Value::as_str).unwrap_or("/api");
    let status = input.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
    let mut headers = HeaderMap::new();
    for (name, value) in string_map(input.get("headers")) {
        headers.insert(
            HeaderName::try_from(name).map_err(|e| e.to_string())?,
            HeaderValue::try_from(value).map_err(|e| e.to_string())?,
        );
    }
    let kind = classify(uri.path(), prefix);
    let got = json!({
        "path": uri.path(),
        "query": redact_query(uri.query().unwrap_or("")),
        "kind": kind.as_str(),
        "level": level_for(kind, status).to_string(),
        "client_ip": log_client_ip(&headers, text(input, "remote_addr")),
    });
    expect_eq("record", got, case.want.clone())
}
