use std::time::Duration;

use sqlx::PgExecutor;
use time::OffsetDateTime;

use super::pending::PendingFuture;
use super::postgres::PostgresPending;
use crate::errors::{Error, Result};

/// How often [`keep_pruned`] prunes when its `every` is zero, as Go `DefaultPruneEvery`.
pub const DEFAULT_PRUNE_EVERY: Duration = Duration::from_secs(3600);

/// How old a closed action gets before [`keep_pruned`] deletes it when its `older_than` is zero,
/// as Go `DefaultPruneAfter`.
pub const DEFAULT_PRUNE_AFTER: Duration = Duration::from_secs(30 * 24 * 3600);

/// The future a [`Pruner`] returns.
pub type PruneFuture<'a> = PendingFuture<'a, u64>;

/// A store that drops the actions it no longer needs, such as [`PostgresPending`], as Go
/// `mcp.Pruner`.
pub trait Pruner: Send + Sync + 'static {
    /// Deletes the actions resolved, or expired, more than `older_than` ago, and reports how
    /// many went.
    fn prune(&self, older_than: Duration) -> PruneFuture<'_>;
}

impl Pruner for PostgresPending {
    fn prune(&self, older_than: Duration) -> PruneFuture<'_> {
        Box::pin(prune_sql(&self.pool, older_than))
    }
}

/// [`Pruner::prune`] through `executor`, in one statement, as Go `mcp.PrunePending`.
pub async fn prune_pending<'c, E: PgExecutor<'c>>(executor: E, older_than: Duration) -> Result<u64> {
    prune_sql(executor, older_than).await
}

/// Prunes `store` at once, then every `every`, until the runtime shuts down, as Go
/// `mcp.KeepPruned`. A zero `every` means [`DEFAULT_PRUNE_EVERY`] and a zero `older_than`
/// [`DEFAULT_PRUNE_AFTER`]. A failure is logged and retried on the next tick.
pub fn keep_pruned<P: Pruner>(store: P, every: Duration, older_than: Duration) -> tokio::task::JoinHandle<()> {
    let every = if every.is_zero() { DEFAULT_PRUNE_EVERY } else { every };
    let older_than = if older_than.is_zero() { DEFAULT_PRUNE_AFTER } else { older_than };
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            prune_once(&store, older_than).await;
        }
    })
}

async fn prune_once<P: Pruner>(store: &P, older_than: Duration) {
    match store.prune(older_than).await {
        Ok(0) => {}
        Ok(count) => tracing::info!(target: "mcp", count, "mcp: pruned pending actions"),
        Err(error) => tracing::warn!(target: "mcp", error = %error, "mcp: pruning pending actions failed"),
    }
}

async fn prune_sql<'c, E: PgExecutor<'c>>(executor: E, older_than: Duration) -> Result<u64> {
    let cutoff = OffsetDateTime::now_utc() - older_than;
    let sql = "DELETE FROM mcp_actions WHERE (resolved_at IS NOT NULL AND resolved_at < $1) OR expires_at < $1";
    let query = sqlx::query(sql).bind(cutoff);
    query
        .execute(executor)
        .await
        .map(|done| done.rows_affected())
        .map_err(|error| Error::internal("failed to prune pending actions", error))
}
