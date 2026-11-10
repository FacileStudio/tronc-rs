use axum::Router;
use serde_json::Value;
use tronc::health::{Check, routes};
use tronc::http::not_found;

use super::assert::expect_answer;
use super::support::{Case, call, request, text};

pub async fn check(case: &Case) -> Result<(), String> {
    let checks = match case.input.get("db_ok").and_then(Value::as_bool) {
        Some(ok) => vec![Check::new(move || async move { ok })],
        None => Vec::new(),
    };
    let router: Router = routes(checks).fallback(not_found);
    let answer = call(router, request(text(&case.input, "method"), text(&case.input, "path"), "")).await;
    expect_answer(&answer, &case.want)
}
