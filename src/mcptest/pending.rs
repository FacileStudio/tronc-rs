use std::collections::HashMap;
use std::sync::Mutex;

use time::OffsetDateTime;

use crate::mcp::Action;

pub(super) struct Row {
    pub action: Action,
    pub resolved: bool,
    pub outcome: String,
}

impl Row {
    pub fn open(&self, user_id: &str, now: OffsetDateTime) -> bool {
        self.action.user_id == user_id && !self.resolved && now < self.action.expires_at
    }
}

/// An in-memory [`crate::mcp::Pending`] for tests. Claims are atomic under one mutex, like the
/// `UPDATE` of [`crate::mcp::PostgresPending`].
#[derive(Default)]
pub struct MemoryPending {
    rows: Mutex<HashMap<String, Row>>,
}

impl MemoryPending {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded outcome of action `id`, for assertions.
    pub fn outcome(&self, id: &str) -> String {
        self.with(|rows| rows.get(id).map(|r| r.outcome.clone()).unwrap_or_default())
    }

    pub(super) fn with<T>(&self, f: impl FnOnce(&mut HashMap<String, Row>) -> T) -> T {
        f(&mut self.rows.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }
}
