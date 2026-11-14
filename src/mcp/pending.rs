use std::future::Future;
use std::pin::Pin;

use serde::Serialize;
use time::OffsetDateTime;

use crate::errors::{Error, Result};

/// A claimed action that is running.
pub const OUTCOME_RUNNING: &str = "running";
/// An action that ran.
pub const OUTCOME_DONE: &str = "done";
/// An action the person declined in their client.
pub const OUTCOME_DECLINED: &str = "declined";
/// An action the person dismissed on the confirm page.
pub const OUTCOME_DISMISSED: &str = "dismissed";
/// An action closed because its target changed after the person was asked.
pub const OUTCOME_STALE: &str = "stale";

/// The refusal for an action that ran, was dismissed, expired or was never this person's.
pub const GONE: &str = "this action was already confirmed, dismissed or expired";

/// The `not_found` error carrying [`GONE`].
pub fn gone() -> Error {
    Error::not_found(GONE)
}

/// One sensitive action waiting for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    /// A random token; it appears in the confirm link.
    pub id: String,
    /// The person who must confirm.
    pub user_id: String,
    /// Names the action, such as `delete_event`; it picks the [`super::Kind`] that runs it.
    pub kind: String,
    /// The app's reference to what the action touches, such as an event id.
    pub target: String,
    /// Fingerprints the target as it was when the person was asked.
    #[serde(skip)]
    pub digest: String,
    /// The question the person was asked, for the confirm page.
    pub summary: String,
    /// The OAuth client that asked.
    pub client_id: String,
    /// When the action was opened.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When the action stops waiting.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

/// The future a [`Pending`] method returns.
pub type PendingFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// The most waiting actions [`super::Confirmations::open`] lists for one person, as Go
/// `mcp.OpenLimit`.
pub const OPEN_LIMIT: usize = 100;

/// Stores actions waiting for a person. `claim` must be atomic: of two claims of one open
/// action, exactly one succeeds. That makes every confirmation single use, whether it comes from
/// the client or from the confirm page.
pub trait Pending: Send + Sync + 'static {
    /// Stores a new action, whose `id`, `created_at` and `expires_at` are set, and returns it.
    /// When the person already has an open action of the same kind on the same target with the
    /// same digest, asked by the same OAuth client, it stores nothing and returns that one, so an
    /// agent that asks twice leaves one entry on the confirm page. Another client of the same
    /// person gets its own action, so consent given through one client never reaches another.
    fn create<'a>(&'a self, action: &'a Action) -> PendingFuture<'a, Action>;

    /// Lists the person's unresolved, unexpired actions, newest first. It may stop after
    /// [`OPEN_LIMIT`] + 1 of them.
    fn open<'a>(&'a self, user_id: &'a str) -> PendingFuture<'a, Vec<Action>>;

    /// Marks one of the person's open actions as running and returns it, or [`gone`].
    fn claim<'a>(&'a self, user_id: &'a str, id: &'a str) -> PendingFuture<'a, Action>;

    /// Records the outcome of a claimed action.
    fn finish<'a>(&'a self, id: &'a str, outcome: &'a str) -> PendingFuture<'a, ()>;

    /// Reopens a claimed action whose run failed.
    fn release<'a>(&'a self, id: &'a str) -> PendingFuture<'a, ()>;

    /// Closes one of the person's open actions without running it, or returns [`gone`].
    fn resolve<'a>(&'a self, user_id: &'a str, id: &'a str, outcome: &'a str) -> PendingFuture<'a, ()>;

    /// Deletes every action of `user_ids`, whatever its state, for an account erasure. Pending
    /// summaries and targets hold the person's content, so they must not outlive the account.
    fn erase<'a>(&'a self, user_ids: &'a [String]) -> PendingFuture<'a, ()>;
}
