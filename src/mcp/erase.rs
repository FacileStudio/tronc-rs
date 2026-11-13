use sqlx::PgExecutor;

use crate::errors::{Error, Result};

/// Deletes every action of `user_ids` from the `mcp_actions` table through `executor`, as Go
/// `mcp.ErasePending`, so an account erasure can run it inside its own transaction
/// (`erase_pending(&mut *tx, &ids)`).
pub async fn erase_pending<'c, E: PgExecutor<'c>>(executor: E, user_ids: &[String]) -> Result<()> {
    sqlx::query("DELETE FROM mcp_actions WHERE user_id = ANY($1)")
        .bind(user_ids)
        .execute(executor)
        .await
        .map(drop)
        .map_err(|error| Error::internal("failed to erase pending actions", error))
}
