use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::body::Body;
use http::Request;
use http::request::Parts;
use serde_json::Value;
use tronc::mcp::{
    self, Annotations, Answer, App, AppConfig, AppSpec, Call, TokenInfo, ToolSpec, Verifier, VerifyError, VerifyFuture,
};

use super::assert::{expect_answer, expect_eq, header};
use super::support::{Case, call, string_map, text};

struct Tokens;

impl Verifier for Tokens {
    fn verify<'a>(&'a self, token: &'a str, _request: &'a Parts) -> VerifyFuture<'a> {
        Box::pin(async move {
            let (user, scopes) = token.split_once('|').filter(|(u, _)| !u.is_empty()).ok_or(VerifyError::Invalid)?;
            Ok(TokenInfo {
                user_id: user.to_owned(),
                scopes: scopes.split(',').map(str::to_owned).collect(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
                extra: HashMap::new(),
            })
        })
    }
}

fn spec(name: &str, title: &str, annotations: Annotations) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        title: title.into(),
        description: format!("{title}."),
        annotations: Some(annotations),
    }
}

fn router(per_minute: u32) -> Router {
    let mut cfg = AppConfig::new("https://notes.example/mcp", "https://sso.example", Arc::new(Tokens));
    cfg.version = "test".into();
    cfg.origins = vec!["https://app.notes.example".into()];
    cfg.per_minute = per_minute;
    let noop = |_: Call, _: HashMap<String, Value>| async { Ok(Answer::Output(HashMap::<String, Value>::new())) };
    let tools = vec![
        mcp::tool(spec("notes_list", "List", Annotations::READ), noop),
        mcp::tool(spec("notes_write", "Write", Annotations::OVERWRITE), noop),
    ];
    let app = App::new(cfg, AppSpec { name: "notes".into(), title: "Notes".into(), tools, ..AppSpec::default() });
    app.expect("app").mount(Router::new()).expect("mount")
}

fn request(input: &Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method(text(input, "method"))
        .uri(format!("https://notes.example{}", text(input, "path")))
        .header("host", "notes.example")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    for (name, value) in string_map(input.get("headers")) {
        builder = builder.header(name, value);
    }
    builder.body(Body::from(text(input, "body").to_owned())).expect("request")
}

pub async fn http_case(case: &Case) -> Result<(), String> {
    let answer = call(router(0), request(&case.input)).await;
    let challenge: Vec<&str> =
        answer.headers.get_all("www-authenticate").iter().filter_map(|v| v.to_str().ok()).collect();
    expect_eq("www-authenticate", challenge.join(", ").as_str(), text(&case.want, "www_authenticate"))?;
    expect_answer(&answer, &case.want)
}

pub async fn rate_limit_case(case: &Case) -> Result<(), String> {
    let per_minute = case.input["per_minute"].as_u64().unwrap_or(0) as u32;
    let router = router(per_minute);
    let users: Vec<String> = serde_json::from_value(case.input["users"].clone()).map_err(|e| e.to_string())?;
    let mut allowed = serde_json::Map::new();
    for user in users {
        let mut passed = 0;
        for _ in 0..case.input["requests_each"].as_u64().unwrap_or(0) {
            let input = serde_json::json!({"method": "GET", "path": "/mcp", "headers": {"Authorization": format!("Bearer {user}|notes.read")}});
            let answer = call(router.clone(), request(&input)).await;
            if answer.status.as_u16() != 429 {
                passed += 1;
                continue;
            }
            expect_eq(
                "refused content type",
                header(&answer.headers, "content-type"),
                text(&case.want, "refused_content_type"),
            )?;
            expect_eq(
                "refused body",
                String::from_utf8_lossy(&answer.body).as_ref(),
                text(&case.want, "refused_body"),
            )?;
            expect_eq("retry-after", !header(&answer.headers, "retry-after").is_empty(), true)?;
        }
        allowed.insert(user, passed.into());
    }
    expect_eq("allowed", &Value::Object(allowed), &case.want["allowed"])
}
