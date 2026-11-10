#![cfg(all(feature = "http", feature = "health", feature = "spa"))]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

fn example_binary() -> PathBuf {
    let deps = std::env::current_exe().expect("test binary");
    deps.parent().and_then(|d| d.parent()).expect("target dir").join("examples").join("minimal")
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").expect("bind").local_addr().expect("addr").port()
}

fn get(port: u16, path: &str) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").ok()?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer).ok()?;
    Some(answer)
}

fn healthcheck(port: u16) -> bool {
    let status = Command::new(example_binary()).arg("healthcheck").env("PORT", port.to_string()).status();
    status.expect("run healthcheck").success()
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn boot(port: u16, client: &std::path::Path) -> Server {
    let child = Command::new(example_binary())
        .env("PORT", port.to_string())
        .env("CLIENT_DIR", client)
        .env("LOG_LEVEL", "error")
        .spawn()
        .expect("spawn the example");
    let server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while get(port, "/health").is_none() {
        assert!(Instant::now() < deadline, "the example never answered");
        std::thread::sleep(Duration::from_millis(50));
    }
    server
}

#[test]
fn the_minimal_example_boots_answers_health_and_serves_the_spa() {
    let client = tempfile::tempdir().expect("client dir");
    std::fs::write(client.path().join("index.html"), "<!doctype html><title>minimal</title>").expect("index");
    let port = free_port();
    assert!(!healthcheck(port), "the probe passed with nothing listening");
    let _server = boot(port, client.path());
    let health = get(port, "/api/health").expect("health");
    assert!(health.starts_with("HTTP/1.1 200") && health.ends_with("{\"status\":\"ok\"}\n"), "{health}");
    assert!(health.to_ascii_lowercase().contains("x-request-id:"));
    let page = get(port, "/sheets/42").expect("spa");
    assert!(page.starts_with("HTTP/1.1 200") && page.contains("<title>minimal</title>"), "{page}");
    assert!(get(port, "/missing.js").expect("asset").starts_with("HTTP/1.1 404"));
    assert!(healthcheck(port), "the probe failed against a healthy server");
}
