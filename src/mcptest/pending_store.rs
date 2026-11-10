use time::OffsetDateTime;

use super::pending::{MemoryPending, Row};
use crate::mcp::{Action, OUTCOME_RUNNING, Pending, PendingFuture, gone};

impl Pending for MemoryPending {
    fn create<'a>(&'a self, action: &'a Action) -> PendingFuture<'a, ()> {
        let row = Row { action: action.clone(), resolved: false, outcome: String::new() };
        self.with(|rows| rows.insert(action.id.clone(), row));
        Box::pin(async { Ok(()) })
    }

    fn open<'a>(&'a self, user_id: &'a str) -> PendingFuture<'a, Vec<Action>> {
        let now = OffsetDateTime::now_utc();
        let mut open: Vec<Action> =
            self.with(|rows| rows.values().filter(|r| r.open(user_id, now)).map(|r| r.action.clone()).collect());
        open.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
        Box::pin(async move { Ok(open) })
    }

    fn claim<'a>(&'a self, user_id: &'a str, id: &'a str) -> PendingFuture<'a, Action> {
        let claimed = self.with(|rows| match rows.get_mut(id) {
            Some(row) if row.open(user_id, OffsetDateTime::now_utc()) => {
                row.resolved = true;
                row.outcome = OUTCOME_RUNNING.to_owned();
                Ok(row.action.clone())
            }
            _ => Err(gone()),
        });
        Box::pin(async move { claimed })
    }

    fn finish<'a>(&'a self, id: &'a str, outcome: &'a str) -> PendingFuture<'a, ()> {
        self.with(|rows| rows.get_mut(id).map(|r| r.outcome = outcome.to_owned()));
        Box::pin(async { Ok(()) })
    }

    fn release<'a>(&'a self, id: &'a str) -> PendingFuture<'a, ()> {
        self.with(|rows| {
            if let Some(row) = rows.get_mut(id).filter(|r| r.outcome == OUTCOME_RUNNING) {
                row.resolved = false;
                row.outcome.clear();
            }
        });
        Box::pin(async { Ok(()) })
    }

    fn resolve<'a>(&'a self, user_id: &'a str, id: &'a str, outcome: &'a str) -> PendingFuture<'a, ()> {
        let resolved = self.with(|rows| match rows.get_mut(id) {
            Some(row) if row.action.user_id == user_id && !row.resolved => {
                row.resolved = true;
                row.outcome = outcome.to_owned();
                Ok(())
            }
            _ => Err(gone()),
        });
        Box::pin(async move { resolved })
    }
}
