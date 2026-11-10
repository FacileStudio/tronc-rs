use std::io::Read;

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::fold::Folded;
use crate::errors::{Error, Result};

/// Decodes exactly one JSON value from a request body, as Go `httpjson.DecodeJSONLimit`.
///
/// The body is capped at `limit` bytes and trailing data is refused. As in Go, object keys
/// match field names case-insensitively when no exact match exists, and a `null` body decodes
/// to the type's empty object. Unknown fields are refused when `T` carries
/// `#[serde(deny_unknown_fields)]`.
pub fn decode<T: DeserializeOwned>(body: &[u8], limit: usize) -> Result<T> {
    if body.len() > limit {
        return Err(Error::too_large("request body too large"));
    }
    strict(body)
}

/// Decodes a gzip-compressed JSON body, as Go `httpjson.DecodeGzipJSONLimit`: the compressed
/// body is capped at `compressed_limit` and the inflated stream at `decompressed_limit`.
pub fn decode_gzip<T: DeserializeOwned>(body: &[u8], compressed_limit: usize, decompressed_limit: usize) -> Result<T> {
    if body.len() > compressed_limit {
        return Err(Error::too_large("request body too large"));
    }
    if body.len() < 10 || body[..3] != [0x1f, 0x8b, 0x08] {
        return Err(Error::invalid("invalid gzip body"));
    }
    let mut inflated = Vec::new();
    let cap = u64::try_from(decompressed_limit).unwrap_or(u64::MAX).saturating_add(1);
    let read = GzDecoder::new(body).take(cap).read_to_end(&mut inflated);
    if inflated.len() > decompressed_limit {
        return Err(Error::too_large("decompressed body too large"));
    }
    read.map_err(|_| Error::invalid("invalid JSON body"))?;
    strict(&inflated)
}

fn strict<T: DeserializeOwned>(body: &[u8]) -> Result<T> {
    let invalid = |_| Error::invalid("invalid JSON body");
    let mut deserializer = serde_json::Deserializer::from_slice(body);
    let value = Value::deserialize(&mut deserializer).map_err(invalid)?;
    deserializer.end().map_err(|_| Error::invalid("request body must contain a single JSON object"))?;
    match T::deserialize(Folded(value.clone())) {
        Ok(decoded) => Ok(decoded),
        Err(_) if value.is_null() => T::deserialize(Folded(Value::Object(Default::default()))).map_err(invalid),
        Err(error) => Err(invalid(error)),
    }
}

/// Panics unless `T` refuses a field it does not declare. Call it once per request type in a
/// test: Rust has no global "disallow unknown fields", so every request type must carry
/// `#[serde(deny_unknown_fields)]`, and this is what proves it does.
pub fn assert_denies_unknown_fields<T: DeserializeOwned>() {
    let probe = br#"{"__tronc_unknown_field_probe":0}"#;
    match serde_json::from_slice::<T>(probe) {
        Err(error) if error.to_string().contains("unknown field") => {}
        _ => panic!("{} accepts unknown fields; add #[serde(deny_unknown_fields)]", std::any::type_name::<T>()),
    }
}
