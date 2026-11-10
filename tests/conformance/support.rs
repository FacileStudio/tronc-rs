use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::Value;
use tower::ServiceExt;

#[derive(Deserialize)]
pub struct Vector {
    pub source: String,
    pub subject: String,
    pub cases: Vec<Case>,
}

#[derive(Deserialize)]
pub struct Case {
    pub name: String,
    pub input: Value,
    pub want: Value,
}

pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

pub fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors")
}

pub fn load_vectors() -> Vec<Vector> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(vectors_dir())
        .expect("tests/vectors")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| serde_json::from_slice(&std::fs::read(path).expect("read vector")).expect("vector envelope"))
        .collect()
}

pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    serde_json::from_value(value.cloned().unwrap_or(Value::Null)).unwrap_or_default()
}

pub fn request(method: &str, uri: &str, remote_addr: &str) -> Request<Body> {
    let mut request = Request::builder().method(method).uri(uri).body(Body::empty()).expect("request");
    if let Ok(peer) = remote_addr.parse::<SocketAddr>() {
        request.extensions_mut().insert(ConnectInfo(peer));
    }
    request
}

pub async fn call(router: Router, request: Request<Body>) -> Answer {
    let response = router.oneshot(request).await.expect("infallible");
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.expect("body").to_bytes().to_vec();
    Answer { status, headers, body }
}
