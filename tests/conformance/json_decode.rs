use std::io::Write;

use axum::Router;
use axum::body::Body;
use axum::routing::post;
use flate2::Compression;
use flate2::write::GzEncoder;
use http::Request;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tronc::json::{Json, MAX_BODY_BYTES, decode, decode_gzip};

use super::assert::expect_eq;
use super::support::{Case, call, text};

#[derive(Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, default)]
struct Sheet {
    name: String,
    count: i64,
}

fn gzip(body: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(body).expect("gzip");
    encoder.finish().expect("gzip")
}

fn outcome(result: tronc::Result<Sheet>) -> Value {
    match result {
        Ok(sheet) => json!({ "status": 200, "decoded": sheet }),
        Err(error) => {
            json!({ "status": error.code.status().as_u16(), "code": error.code.as_str(), "message": error.message })
        }
    }
}

async fn through_extractor(body: &str, content_type: &str) -> Value {
    let router = Router::new().route("/", post(|Json(sheet): Json<Sheet>| async move { Json(sheet) }));
    let request =
        Request::post("/").header("content-type", content_type).body(Body::from(body.to_owned())).expect("request");
    let answer = call(router, request).await;
    let body: Value = serde_json::from_slice(&answer.body).unwrap_or(Value::Null);
    match answer.status.as_u16() {
        200 => json!({ "status": 200, "decoded": body }),
        status => json!({ "status": status, "code": body["error"]["code"], "message": body["error"]["message"] }),
    }
}

pub async fn check(case: &Case) -> Result<(), String> {
    let input = &case.input;
    let body = text(input, "body").as_bytes();
    let limit = input.get("limit").and_then(Value::as_u64).unwrap_or(0) as usize;
    let got = if input.get("gzip").and_then(Value::as_bool) == Some(true) {
        let compressed = input.get("compressed").and_then(Value::as_bool) == Some(true);
        let wire = if compressed { gzip(body) } else { body.to_vec() };
        outcome(decode_gzip(&wire, MAX_BODY_BYTES, if limit == 0 { MAX_BODY_BYTES } else { limit }))
    } else if limit == 0 {
        let direct = outcome(decode(body, MAX_BODY_BYTES));
        expect_eq(
            "extractor",
            through_extractor(text(input, "body"), text(input, "content_type")).await,
            direct.clone(),
        )?;
        direct
    } else {
        outcome(decode(body, limit))
    };
    expect_eq("outcome", got, case.want.clone())
}
