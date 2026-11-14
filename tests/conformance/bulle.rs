use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::response::Response;
use http::{Method, Uri};
use serde_json::{Value, json};
use tronc::bulle::{Client, CompleteRequest, Error, Request};

use super::assert::expect_eq;
use super::support::{Case, text};

async fn serve(router: Router) -> Client {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, router).await });
    Client::new(&url).expect("client")
}

fn classify(result: Result<(), Error>) -> Value {
    match result {
        Ok(()) => json!({"kind": "ok"}),
        Err(Error::PlanRequired(plan)) => {
            let mut want = json!({"kind": "plan_required"});
            let fields = [("message", plan.message), ("upgrade_url", plan.upgrade_url), ("key", plan.key)];
            for (name, value) in fields.into_iter().filter(|(_, v)| !v.is_empty()) {
                want[name] = value.into();
            }
            if plan.limit != 0 {
                want["limit"] = plan.limit.into();
            }
            want
        }
        Err(Error::Status { status, code, message, retry_after }) => {
            let mut want = json!({"kind": "status", "status": status});
            for (name, value) in [("code", code), ("message", message)].into_iter().filter(|(_, v)| !v.is_empty()) {
                want[name] = value.into();
            }
            if !retry_after.is_zero() {
                want["retry_after_seconds"] = retry_after.as_secs().into();
            }
            want
        }
        Err(Error::Unreachable(_)) => json!({"kind": "unreachable"}),
        Err(_) => json!({"kind": "other"}),
    }
}

pub async fn errors_case(case: &Case) -> Result<(), String> {
    let (status, body, location, retry_after) = (
        case.input["status"].as_u64().unwrap_or(500) as u16,
        text(&case.input, "body").to_owned(),
        text(&case.input, "location").to_owned(),
        text(&case.input, "retry_after").to_owned(),
    );
    let answer = move || {
        let mut response = Response::builder().status(status).header("content-type", "application/json");
        if !location.is_empty() {
            response = response.header("location", location.clone());
        }
        if !retry_after.is_empty() {
            response = response.header("retry-after", retry_after.clone());
        }
        let response = response.body(Body::from(body.clone())).expect("response");
        async move { response }
    };
    let client = serve(Router::new().fallback(answer)).await;
    let request = CompleteRequest { prompt: "x".into(), ..CompleteRequest::default() };
    let got = classify(client.complete("tok", &request).await.map(drop));
    expect_eq("outcome", &got, &case.want)
}

pub async fn paths_case(case: &Case) -> Result<(), String> {
    let seen = Arc::new(Mutex::new(Value::Null));
    let record = seen.clone();
    let router = Router::new().fallback(move |uri: Uri| {
        *record.lock().expect("lock") = json!({"path": uri.path(), "query": uri.query().unwrap_or("")});
        async { http::StatusCode::NO_CONTENT }
    });
    let client = serve(router).await;
    let segments: Vec<String> = serde_json::from_value(case.input["segments"].clone()).map_err(|e| e.to_string())?;
    let query: BTreeMap<String, String> =
        serde_json::from_value(case.input.get("query").cloned().unwrap_or(json!({}))).map_err(|e| e.to_string())?;
    let mut request = Request::new(Method::GET, segments);
    for (key, value) in &query {
        request = request.query(key, value);
    }
    let got = match client.call::<(), _>("tok", request).await {
        Err(Error::BadSegment) => json!({"error": "bad_segment"}),
        Err(other) => return Err(other.to_string()),
        Ok(()) => {
            let mut got = seen.lock().expect("lock").clone();
            if got["query"] == "" {
                got.as_object_mut().map(|o| o.remove("query"));
            }
            got
        }
    };
    expect_eq("request", &got, &case.want)
}
