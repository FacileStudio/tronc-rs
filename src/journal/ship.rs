use std::time::{Duration, Instant};

use serde_json::json;

use super::worker::Shared;
use super::{Entry, MAX_SERVER_BATCH};

const MAX_RETRY_AFTER: Duration = Duration::from_secs(300);

impl Shared {
    pub(super) fn flush(&self, force: bool) -> bool {
        let batch: Vec<Entry> = {
            let mut state = self.lock();
            if !force && state.retry_at.is_some_and(|at| Instant::now() < at) {
                return false;
            }
            let size = state.buffer.len().min(MAX_SERVER_BATCH);
            state.buffer.drain(..size).collect()
        };
        if batch.is_empty() {
            return false;
        }
        if self.ship(&batch) {
            self.requeue(batch);
            return false;
        }
        true
    }

    fn requeue(&self, batch: Vec<Entry>) {
        let mut state = self.lock();
        for entry in batch.into_iter().rev() {
            state.buffer.push_front(entry);
        }
        let over = state.buffer.len().saturating_sub(self.config.buffer_cap);
        state.buffer.drain(..over);
        state.dropped += over;
    }

    pub(super) fn ship(&self, entries: &[Entry]) -> bool {
        let body = json!({ "entries": entries }).to_string();
        let response = self
            .agent
            .post(format!("{}/ingest", self.config.url))
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send(body);
        let Ok(response) = response else { return true };
        let status = response.status().as_u16();
        if status == 429 {
            let delay = retry_delay(response.headers().get("retry-after").and_then(|v| v.to_str().ok()));
            self.lock().retry_at = Some(Instant::now() + delay);
            return true;
        }
        status >= 500
    }
}

fn retry_delay(header: Option<&str>) -> Duration {
    match header.and_then(|value| value.trim().parse::<u64>().ok()) {
        Some(seconds) if seconds > 0 => Duration::from_secs(seconds).min(MAX_RETRY_AFTER),
        _ => Duration::ZERO,
    }
}
