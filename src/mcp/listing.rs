use super::confirmations::Confirmations;
use super::pending::{Action, OPEN_LIMIT};
use crate::errors::Result;

impl Confirmations {
    /// Lists the person's waiting actions, newest first, at most [`OPEN_LIMIT`] of them.
    pub async fn open(&self, user_id: &str) -> Result<Vec<Action>> {
        Ok(self.list(user_id).await?.0)
    }

    /// [`Self::open`] that also reports whether more than [`OPEN_LIMIT`] actions wait, as Go
    /// `Confirmations.List`; the oldest are left out.
    pub async fn list(&self, user_id: &str) -> Result<(Vec<Action>, bool)> {
        let mut actions = self.cfg.store.open(user_id).await?;
        let truncated = actions.len() > OPEN_LIMIT;
        actions.truncate(OPEN_LIMIT);
        Ok((actions, truncated))
    }
}
