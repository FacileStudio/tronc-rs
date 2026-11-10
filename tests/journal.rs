#![cfg(feature = "journal")]

#[path = "journal/fake.rs"]
mod fake;

use std::time::Duration;

use fake::fake_journal;
use serde_json::{Map, json};
use tronc::journal::{Client, Config};

fn client(url: &str) -> Client {
    Client::new(Config { flush_interval: Duration::from_secs(60), ..Config::new(url, "journal_test_token") })
}

#[test]
fn close_ships_the_buffer_in_journals_wire_format() {
    let (url, received) = fake_journal(vec![202]);
    let client = client(&url);
    let mut meta = Map::new();
    meta.insert("request_id".into(), json!("abc"));
    client.log("warn", "slow query", meta);
    client.close();
    let captured = received.recv_timeout(Duration::from_secs(5)).expect("a request");
    assert_eq!(captured.request_line, "POST /ingest HTTP/1.1");
    assert_eq!(captured.authorization, "Bearer journal_test_token");
    let entry = &captured.body["entries"][0];
    assert_eq!((entry["level"].as_str(), entry["message"].as_str()), (Some("warn"), Some("slow query")));
    assert_eq!(entry["meta"], json!({ "request_id": "abc" }));
    assert!(entry.get("app").is_none(), "an empty app must be omitted for per-app keys");
    assert!(entry["ts"].as_str().is_some_and(|ts| ts.ends_with('Z')));
    assert_eq!(client.dropped(), 0);
}

#[test]
fn a_server_error_keeps_the_batch_and_close_counts_it_dropped() {
    let (url, received) = fake_journal(vec![503; 8]);
    let client = client(&url);
    client.log("info", "one", Map::new());
    client.log("info", "two", Map::new());
    client.close();
    assert!(received.recv_timeout(Duration::from_secs(5)).is_ok());
    assert_eq!(client.dropped(), 2);
}

#[test]
fn a_client_error_drops_the_batch_without_retrying() {
    let (url, received) = fake_journal(vec![400, 400]);
    let client = client(&url);
    client.log("info", "bad", Map::new());
    client.close();
    assert!(received.recv_timeout(Duration::from_secs(5)).is_ok());
    assert!(received.recv_timeout(Duration::from_millis(200)).is_err(), "a 4xx batch was retried");
    assert_eq!(client.dropped(), 0);
}

#[test]
fn the_buffer_drops_the_oldest_entries_past_its_cap() {
    let (url, _received) = fake_journal(vec![202]);
    let client = Client::new(Config {
        buffer_cap: 2,
        max_batch: 1000,
        flush_interval: Duration::from_secs(60),
        ..Config::new(&url, "t")
    });
    for message in ["a", "b", "c"] {
        client.log("info", message, Map::new());
    }
    assert_eq!(client.dropped(), 1);
    client.close();
}
