//! Bounded, strict JSON request bodies and Go-compatible JSON responses.

mod decode;
mod encode;
mod fold;
mod fold_access;

use axum::body::to_bytes;
use axum::extract::{FromRequest, Request};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use http_body_util::LengthLimitError;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub use decode::{assert_denies_unknown_fields, decode, decode_gzip};
pub use encode::{encode, write};

use crate::errors::Error;

/// The default body cap, 1 MiB, as Go `httpjson.MaxBodyBytes`.
pub const MAX_BODY_BYTES: usize = 1 << 20;

/// A JSON request body or response.
///
/// As an extractor it caps the body at [`MAX_BODY_BYTES`], ignores `Content-Type` as Go does,
/// refuses trailing data, and answers every failure with the suite envelope. It never inflates
/// gzip: call [`decode_gzip`] on the raw body for the one route that takes it, as Go does. As a
/// response it writes `200` with the bytes Go tronc writes.
#[derive(Debug, Clone, Copy, Default)]
pub struct Json<T>(pub T);

/// [`Json`] with a per-route body cap, as Go `DecodeJSONLimit`: `JsonLimit<Batch, { 8 << 20 }>`.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonLimit<T, const LIMIT: usize>(pub T);

async fn extract<T: DeserializeOwned>(request: Request, limit: usize) -> Result<T, Error> {
    let body = to_bytes(request.into_body(), limit).await.map_err(read_error)?;
    decode(&body, limit)
}

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Json<T> {
    type Rejection = Error;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        extract(request, MAX_BODY_BYTES).await.map(Json)
    }
}

impl<T: DeserializeOwned, S: Send + Sync, const LIMIT: usize> FromRequest<S> for JsonLimit<T, LIMIT> {
    type Rejection = Error;

    async fn from_request(request: Request, _state: &S) -> Result<Self, Self::Rejection> {
        extract(request, LIMIT).await.map(JsonLimit)
    }
}

fn read_error(error: axum::Error) -> Error {
    if error.into_inner().is::<LengthLimitError>() {
        return Error::too_large("request body too large");
    }
    Error::invalid("invalid JSON body")
}

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        write(StatusCode::OK, &self.0)
    }
}
