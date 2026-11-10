use sqlx::PgPool;

use super::pending::{Action, Pending, PendingFuture};
use super::postgres_sql as sql;

/// The migration that creates the `mcp_actions` table [`PostgresPending`] uses. Copy
/// `migrations/mcp/0001_mcp_actions.sql` into the app's own migrations under its next number;
/// the SQL is idempotent and additive.
pub const PENDING_SCHEMA: &str = include_str!("../../migrations/mcp/0001_mcp_actions.sql");

/// [`Pending`] on the `mcp_actions` table of [`PENDING_SCHEMA`].
#[derive(Debug, Clone)]
pub struct PostgresPending {
    /// The app's pool.
    pub pool: PgPool,
}

impl PostgresPending {
    /// A store on `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Pending for PostgresPending {
    fn create<'a>(&'a self, action: &'a Action) -> PendingFuture<'a, ()> {
        Box::pin(sql::create(&self.pool, action))
    }

    fn open<'a>(&'a self, user_id: &'a str) -> PendingFuture<'a, Vec<Action>> {
        Box::pin(sql::open(&self.pool, user_id))
    }

    fn claim<'a>(&'a self, user_id: &'a str, id: &'a str) -> PendingFuture<'a, Action> {
        Box::pin(sql::claim(&self.pool, user_id, id))
    }

    fn finish<'a>(&'a self, id: &'a str, outcome: &'a str) -> PendingFuture<'a, ()> {
        Box::pin(sql::finish(&self.pool, id, outcome))
    }

    fn release<'a>(&'a self, id: &'a str) -> PendingFuture<'a, ()> {
        Box::pin(sql::release(&self.pool, id))
    }

    fn resolve<'a>(&'a self, user_id: &'a str, id: &'a str, outcome: &'a str) -> PendingFuture<'a, ()> {
        Box::pin(sql::resolve(&self.pool, user_id, id, outcome))
    }
}
