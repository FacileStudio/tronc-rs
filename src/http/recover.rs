use std::any::Any;
use std::panic::AssertUnwindSafe;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use futures_util::FutureExt;
use http::StatusCode;
use serde_json::json;

use super::request_id::RequestId;
use crate::json;

/// Turns a panic below it into a logged 500 carrying the suite envelope.
pub async fn recover(request: Request, next: Next) -> Response {
    let request_id = request.extensions().get::<RequestId>().map(|id| id.0.clone()).unwrap_or_default();
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    match AssertUnwindSafe(next.run(request)).catch_unwind().await {
        Ok(response) => response,
        Err(payload) => {
            tracing::error!(request_id, method, path, panic = describe(payload.as_ref()), "panic recovered");
            let body = json!({ "error": { "code": "internal", "message": "internal server error" } });
            json::write(StatusCode::INTERNAL_SERVER_ERROR, &body)
        }
    }
}

fn describe(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_owned())
}
