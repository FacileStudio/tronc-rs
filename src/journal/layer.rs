use std::sync::Arc;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;

use super::Entry;
use super::fields::Fields;
use super::worker::Shared;

/// A `tracing` layer that buffers every event it sees for Journal. [`crate::logger::init`]
/// installs it behind the same level filter as stdout.
#[derive(Debug, Clone)]
pub struct JournalLayer {
    shared: Arc<Shared>,
}

impl JournalLayer {
    pub(super) fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for JournalLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.shared.enqueue(Entry {
            app: self.shared.app().to_owned(),
            level: level_name(*event.metadata().level()).to_owned(),
            message: fields.message,
            ts: now_rfc3339(),
            meta: fields.meta,
        });
    }
}

/// Journal's name for a `tracing` level.
pub fn level_name(level: Level) -> &'static str {
    match level {
        Level::TRACE | Level::DEBUG => "debug",
        Level::INFO => "info",
        Level::WARN => "warn",
        Level::ERROR => "error",
    }
}

/// The current UTC time in RFC 3339 with nanoseconds, as Go's `time.RFC3339Nano`.
pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default()
}
