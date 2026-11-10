#![cfg(feature = "mcp")]

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::Router;
use axum::body::Body;
use http::request::Parts;
use http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;
use tronc::http::RateLimitConfig;
use tronc::mcp::{Options, Server, TokenInfo, Verifier, VerifyError, VerifyFuture};

struct Tokens;

impl Verifier for Tokens {
    fn verify<'a>(&'a self, token: &'a str, _request: &'a Parts) -> VerifyFuture<'a> {
        Box::pin(async move {
            let in_an_hour = Some(SystemTime::now() + Duration::from_secs(3600));
            let read = vec!["app.read".to_owned()];
            match token {
                "unavailable" => Err(VerifyError::Unavailable),
                "expired" => Ok(TokenInfo {
                    user_id: "1".into(),
                    scopes: read,
                    expires_at: Some(SystemTime::UNIX_EPOCH),
                    ..TokenInfo::default()
                }),
                "forever" => Ok(TokenInfo { user_id: "1".into(), scopes: read, ..TokenInfo::default() }),
                "noread" => Ok(TokenInfo { user_id: "1".into(), expires_at: in_an_hour, ..TokenInfo::default() }),
                user if user.starts_with('u') => {
                    Ok(TokenInfo { user_id: user.into(), scopes: read, expires_at: in_an_hour, ..TokenInfo::default() })
                }
                _ => Err(VerifyError::Invalid),
            }
        })
    }
}

fn router() -> Router {
    let mut options = Options::new("app", "https://app.example/mcp", "https://issuer.example", Arc::new(Tokens));
    options.rate_limit = RateLimitConfig { limit: 0.001, burst: 1, ..RateLimitConfig::default() };
    Server::new(options).expect("server").router()
}

async fn send(
    router: &Router,
    method: Method,
    path: &str,
    auth: Option<&str>,
) -> (StatusCode, http::HeaderMap, String) {
    let mut request = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json");
    request = request.header(header::ACCEPT, "application/json, text/event-stream").header(header::HOST, "app.example");
    if let Some(auth) = auth {
        request = request.header(header::AUTHORIZATION, auth);
    }
    let body = Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}"#);
    let response = router.clone().oneshot(request.body(body).expect("request")).await.expect("response");
    let (status, headers) = (response.status(), response.headers().clone());
    let body = response.into_body().collect().await.expect("body").to_bytes();
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

fn challenge(headers: &http::HeaderMap) -> &str {
    headers.get(header::WWW_AUTHENTICATE).and_then(|v| v.to_str().ok()).unwrap_or("")
}

#[tokio::test]
async fn refusals_carry_the_rfc_6750_challenge() {
    let router = router();
    let meta = "resource_metadata=\"https://app.example/.well-known/oauth-protected-resource/mcp\", scope=\"app.read\"";
    let (status, headers, body) = send(&router, Method::POST, "/mcp", None).await;
    assert_eq!((status, body.as_str()), (StatusCode::UNAUTHORIZED, "no bearer token\n"));
    assert_eq!(challenge(&headers), format!("Bearer {meta}"));
    let (status, headers, _) = send(&router, Method::POST, "/mcp", Some("Bearer nope")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(challenge(&headers), format!("Bearer error=\"invalid_token\", {meta}"));
    let (status, headers, body) = send(&router, Method::POST, "/mcp", Some("Bearer noread")).await;
    assert_eq!((status, body.as_str()), (StatusCode::FORBIDDEN, "insufficient scope\n"));
    assert_eq!(challenge(&headers), format!("Bearer error=\"insufficient_scope\", {meta}"));
    let (status, _, body) = send(&router, Method::POST, "/mcp", Some("Bearer expired")).await;
    assert_eq!((status, body.as_str()), (StatusCode::UNAUTHORIZED, "token expired\n"));
    let (status, _, body) = send(&router, Method::POST, "/mcp", Some("Bearer forever")).await;
    assert_eq!((status, body.as_str()), (StatusCode::UNAUTHORIZED, "token missing expiration\n"));
    let (status, headers, _) = send(&router, Method::POST, "/mcp", Some("Bearer unavailable")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(challenge(&headers), "", "a failure that is not a verdict carries no challenge");
}

#[tokio::test]
async fn the_rate_limit_counts_per_person() {
    let router = router();
    let first = send(&router, Method::POST, "/mcp", Some("Bearer u1")).await;
    assert_eq!(first.0, StatusCode::OK, "{}", first.2);
    assert_eq!(send(&router, Method::POST, "/mcp", Some("Bearer u1")).await.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(send(&router, Method::POST, "/mcp", Some("Bearer u2")).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_metadata_is_served_at_both_well_known_paths() {
    let router = router();
    for path in ["/.well-known/oauth-protected-resource/mcp", "/.well-known/oauth-protected-resource"] {
        let (status, headers, body) = send(&router, Method::GET, path, None).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN).expect("cors"), "*");
        let document: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert_eq!(
            document,
            serde_json::json!({
                "resource": "https://app.example/mcp",
                "authorization_servers": ["https://issuer.example"],
                "scopes_supported": ["app.read", "app.write"],
                "bearer_methods_supported": ["header"],
                "resource_signing_alg_values_supported": ["RS256"],
            })
        );
    }
    let path = "/.well-known/oauth-protected-resource/mcp";
    assert_eq!(send(&router, Method::OPTIONS, path, None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&router, Method::POST, path, None).await.0, StatusCode::METHOD_NOT_ALLOWED);
}
