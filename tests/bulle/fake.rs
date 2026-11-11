use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::response::Response;
use http::{HeaderMap, Method, Uri};
use tronc::bulle::Client;

#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub query: String,
    pub auth: String,
    pub body: String,
    pub requests: usize,
}

pub type Answer = Arc<dyn Fn(usize) -> (u16, String) + Send + Sync>;

pub struct Fake {
    pub url: String,
    seen: Arc<Mutex<Seen>>,
}

impl Fake {
    pub fn seen(&self) -> Seen {
        self.seen.lock().expect("lock").clone()
    }
}

#[derive(Clone)]
struct Shared {
    seen: Arc<Mutex<Seen>>,
    answer: Answer,
    delay: Duration,
}

async fn handle(State(shared): State<Shared>, method: Method, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    let requests = {
        let mut last = shared.seen.lock().expect("lock");
        *last = Seen {
            method: method.to_string(),
            path: uri.path().to_owned(),
            query: uri.query().unwrap_or("").to_owned(),
            auth: headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned(),
            body: String::from_utf8_lossy(&body).into_owned(),
            requests: last.requests + 1,
        };
        last.requests
    };
    tokio::time::sleep(shared.delay).await;
    let (status, body) = (shared.answer)(requests);
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .expect("response")
}

pub async fn fake(answer: Answer, delay: Duration) -> Fake {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let app = Router::new().fallback(handle).with_state(Shared { seen: seen.clone(), answer, delay });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    Fake { url, seen }
}

pub fn fixed(status: u16, body: &str) -> Answer {
    let body = body.to_owned();
    Arc::new(move |_| (status, body.clone()))
}

pub fn client(fake: &Fake) -> Client {
    Client::new(&format!("{}/", fake.url)).expect("client")
}
