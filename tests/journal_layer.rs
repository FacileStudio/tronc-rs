#![cfg(feature = "journal")]

#[path = "journal/fake.rs"]
mod fake;

use std::time::Duration;

use serde_json::json;
use tronc::journal::Config;
use tronc::logger::{self, init};

#[test]
fn the_logger_ships_events_above_its_level_to_journal() {
    let (url, received) = fake::fake_journal(vec![202]);
    let journal = Config { flush_interval: Duration::from_secs(60), ..Config::new(&url, "t") };
    let guard = init(logger::Config { level: "info".into(), journal: Some(journal) });
    tracing::debug!("filtered out");
    tracing::warn!(rows = 3_u64, table = "sheets", "slow query");
    drop(guard);
    let captured = received.recv_timeout(Duration::from_secs(5)).expect("a batch");
    assert_eq!(captured.request_line, "POST /ingest HTTP/1.1");
    assert_eq!(captured.authorization, "Bearer t");
    let entries = captured.body["entries"].as_array().cloned().unwrap_or_default();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0]["level"], "warn");
    assert_eq!(entries[0]["message"], "slow query");
    assert_eq!(entries[0]["meta"], json!({ "rows": 3, "table": "sheets" }));
}
