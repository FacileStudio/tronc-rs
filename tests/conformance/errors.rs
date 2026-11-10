use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use serde_json::Value;
use tronc::Error;
use tronc::errors::{envelope_response, write_error};

use super::assert::expect_answer;
use super::support::{Answer, Case, text};

#[derive(Debug, thiserror::Error)]
#[error("loading the space: {0}")]
struct Wrapped(#[source] Error);

fn construct(name: &str, message: &str, cause: &str) -> Option<Error> {
    let error = match name {
        "Invalid" => Error::invalid(message),
        "Unauthorized" => Error::unauthorized(message),
        "PaymentRequired" => Error::payment_required(message),
        "Forbidden" => Error::forbidden(message),
        "NotFound" => Error::not_found(message),
        "Conflict" => Error::conflict(message),
        "Failed" => Error::failed(message),
        "TooLarge" => Error::too_large(message),
        "RateLimited" => Error::rate_limited(message),
        "NotAllowed" => Error::not_allowed(message),
        "Unavailable" => Error::unavailable(message),
        "Internal" => Error::internal(message, cause.to_owned()),
        _ => return None,
    };
    Some(error)
}

fn respond(input: &Value) -> Response {
    let message = text(input, "message");
    if input.get("raw").and_then(Value::as_bool) == Some(true) {
        return write_error(&std::io::Error::other(message.to_owned()));
    }
    let Some(mut error) = construct(text(input, "constructor"), message, text(input, "cause")) else {
        return envelope_response(text(input, "code"), message, None);
    };
    if let Some(url) = input.get("upgrade_url").and_then(Value::as_str) {
        error = error.with_upgrade_url(url);
    }
    if input.get("wrapped").and_then(Value::as_bool) == Some(true) {
        return write_error(&Wrapped(error));
    }
    error.into_response()
}

pub async fn check(case: &Case) -> Result<(), String> {
    let response = respond(&case.input);
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.map_err(|e| e.to_string())?.to_bytes().to_vec();
    expect_answer(&Answer { status, headers, body }, &case.want)
}
