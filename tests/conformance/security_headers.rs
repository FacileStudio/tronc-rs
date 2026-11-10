use axum::Router;
use axum::routing::get;
use http::HeaderValue;
use serde::Deserialize;
use serde_json::Value;
use tronc::http::{Config, SecurityConfig, Sources, Tls, layers};

use super::assert::{expect_eq, header};
use super::support::{Case, call, request, string_map, text};

const CHECKED: [&str; 8] = [
    "content-security-policy",
    "content-security-policy-report-only",
    "cross-origin-opener-policy",
    "permissions-policy",
    "referrer-policy",
    "strict-transport-security",
    "x-content-type-options",
    "x-frame-options",
];

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct Input {
    csp_enforce: bool,
    csp_sources: Sources,
    csp_report_uri: String,
    allow: Vec<String>,
    cross_origin_opener_policy: String,
    hsts_exclude_subdomains: bool,
}

fn security(config: &Value) -> Result<SecurityConfig, String> {
    let input: Input = serde_json::from_value(config.clone()).map_err(|e| format!("config: {e}"))?;
    Ok(SecurityConfig {
        csp_enforce: input.csp_enforce,
        csp_sources: input.csp_sources,
        csp_report_uri: input.csp_report_uri,
        allow: input.allow,
        cross_origin_opener_policy: input.cross_origin_opener_policy,
        hsts_exclude_subdomains: input.hsts_exclude_subdomains,
        trusted: None,
    })
}

pub async fn check(case: &Case) -> Result<(), String> {
    let config = Config { security: security(&case.input["config"])?, ..Config::default() };
    let chain = match (layers(config), case.want.get("panic").and_then(Value::as_str)) {
        (Err(reason), Some(panic)) => return expect_eq("refusal", reason.as_str(), panic),
        (Err(reason), None) => return Err(format!("refused: {reason}")),
        (Ok(_), Some(panic)) => return Err(format!("accepted a config Go refuses with {panic:?}")),
        (Ok(chain), None) => chain,
    };
    let router = Router::new().route("/", get(|| async { "page" })).layer(chain);
    let mut request = request("GET", "/", text(&case.input, "remote_addr"));
    if case.input.get("tls").and_then(Value::as_bool) == Some(true) {
        request.extensions_mut().insert(Tls);
    }
    for proto in case.input.get("forwarded_proto").and_then(Value::as_array).into_iter().flatten() {
        let value = HeaderValue::from_str(proto.as_str().unwrap_or("")).map_err(|e| e.to_string())?;
        request.headers_mut().append("x-forwarded-proto", value);
    }
    let answer = call(router, request).await;
    let want: std::collections::BTreeMap<String, String> =
        string_map(case.want.get("headers")).into_iter().map(|(k, v)| (k.to_ascii_lowercase(), v)).collect();
    for name in CHECKED {
        expect_eq(name, header(&answer.headers, name), want.get(name).map_or("", String::as_str))?;
    }
    Ok(())
}
