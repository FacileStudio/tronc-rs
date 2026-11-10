//! Ships log records to Journal, a port of `Journal/sdk/journal`.
//!
//! Records are buffered and posted as `POST {url}/ingest` with `{"entries":[...]}` and
//! `Authorization: Bearer <token>`. Shipping is best effort and never blocks the caller: a batch
//! is retried on 429, 5xx and network errors, dropped on any other 4xx, and the oldest entries
//! are dropped when the buffer overflows while Journal is unreachable.

mod fields;
mod layer;
mod ship;
mod worker;

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};

pub use layer::JournalLayer;
use worker::Shared;

/// The largest batch Journal accepts in one request.
pub const MAX_SERVER_BATCH: usize = 1000;

/// Configures a [`Client`]. `url` and `token` are required; the rest have Journal's defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The Journal API base URL, as in `http://journal-api:4010`.
    pub url: String,
    /// A per-app API key, or the legacy ingest token.
    pub token: String,
    /// The source app name. Leave empty with a per-app key: the server fills it in.
    pub app: String,
    /// How often buffered entries are shipped. Default 2 s.
    pub flush_interval: Duration,
    /// An early flush starts when the buffer reaches this size. Default 200, at most 1000.
    pub max_batch: usize,
    /// The buffer bound; the oldest entries are dropped beyond it. Default 5000.
    pub buffer_cap: usize,
}

impl Config {
    /// A configuration with Journal's defaults.
    pub fn new(url: &str, token: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_owned(),
            token: token.to_owned(),
            app: String::new(),
            flush_interval: Duration::from_secs(2),
            max_batch: 200,
            buffer_cap: 5000,
        }
    }
}

/// One log entry as `POST /ingest` accepts it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    /// The source app, omitted when empty.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub app: String,
    /// `debug`, `info`, `warn` or `error`.
    pub level: String,
    /// The message.
    pub message: String,
    /// RFC 3339 UTC timestamp with nanoseconds.
    pub ts: String,
    /// Structured fields, omitted when empty.
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub meta: Map<String, Value>,
}

/// Ships entries to Journal from a background thread. Call [`Client::close`] to flush and stop.
#[derive(Debug, Clone)]
pub struct Client {
    shared: Arc<Shared>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Client {
    /// Starts a client and its shipper thread.
    pub fn new(config: Config) -> Self {
        let shared = Arc::new(Shared::new(config));
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new().name("tronc-journal".into()).spawn(move || worker.run()).ok();
        Self { shared, thread: Arc::new(Mutex::new(thread)) }
    }

    /// Buffers one entry, stamped now.
    pub fn log(&self, level: &str, message: &str, meta: Map<String, Value>) {
        self.shared.enqueue(Entry {
            app: self.shared.app().to_owned(),
            level: level.to_owned(),
            message: message.to_owned(),
            ts: layer::now_rfc3339(),
            meta,
        });
    }

    /// How many entries were discarded, by overflow or because `close` could not deliver them.
    pub fn dropped(&self) -> usize {
        self.shared.dropped()
    }

    /// The `tracing` layer that feeds this client.
    pub fn layer(&self) -> JournalLayer {
        JournalLayer::new(Arc::clone(&self.shared))
    }

    /// Drains the buffer (best effort, bounded) and stops the shipper. Idempotent.
    pub fn close(&self) {
        self.shared.stop();
        let handle = self.thread.lock().ok().and_then(|mut slot| slot.take());
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}
