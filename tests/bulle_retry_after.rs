#![cfg(feature = "bulle")]

use std::time::{Duration, SystemTime};

use axum::body::Body;
use axum::response::Response;
use tronc::bulle::{Client, Error};

async fn refused(status: u16, header: &str) -> Error {
    let header = header.to_owned();
    let answer = move || {
        let mut response = Response::builder().status(status);
        if !header.is_empty() {
            response = response.header("retry-after", header.clone());
        }
        let body = r#"{"error":{"code":"rate_limited","message":"later"}}"#;
        std::future::ready(response.body(Body::from(body)).expect("response"))
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}/", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, axum::Router::new().fallback(answer)).await });
    Client::new(&url).expect("client").generate("tok", "opus", "").await.expect_err("refused")
}

#[tokio::test]
async fn status_errors_carry_retry_after() {
    let later = httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(90));
    let (s, day) = (Duration::from_secs, 24 * 3600);
    let cases = [
        ("120", 429, s(120), s(120), false),
        (later.as_str(), 429, s(80), s(91), false),
        ("", 429, s(0), s(0), false),
        ("soon", 429, s(0), s(0), false),
        ("Wed, 21 Oct 2015 07:28:00 GMT", 429, s(0), s(0), false),
        ("99999999999999", 429, s(day), s(day), false),
        ("30", 503, s(30), s(30), true),
        ("30", 409, s(0), s(0), false),
    ];
    for (header, status, min, max, unreachable) in cases {
        let err = refused(status, header).await;
        assert_eq!(err.status(), Some(status), "{header}: {err:?}");
        assert!((min..=max).contains(&err.retry_after()), "{header}: {err:?}");
        assert_eq!(err.is_unreachable(), unreachable, "{header}: {err:?}");
    }
}

#[tokio::test]
async fn a_503_without_retry_after_stays_unreachable() {
    let err = refused(503, "").await;
    assert!(matches!(err, Error::Unreachable(_)), "{err:?}");
}
