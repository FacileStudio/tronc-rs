use time::OffsetDateTime;

use super::pending::{MemoryPending, Row};
use crate::mcp::{Action, OPEN_LIMIT, OUTCOME_RUNNING, Pending, PendingFuture, gone};

impl Pending for MemoryPending {
    fn create<'a>(&'a self, action: &'a Action) -> PendingFuture<'a, Action> {
        let now = OffsetDateTime::now_utc();
        let stored = self.with(|rows| {
            let same = |r: &&Row| {
                r.open(&action.user_id, now)
                    && (&r.action.kind, &r.action.target, &r.action.digest, &r.action.client_id)
                        == (&action.kind, &action.target, &action.digest, &action.client_id)
            };
            if let Some(found) =
                newest(rows.values().filter(same).map(|r| r.action.clone()).collect()).into_iter().next()
            {
                return found;
            }
            rows.insert(action.id.clone(), Row { action: action.clone(), resolved: false, outcome: String::new() });
            action.clone()
        });
        Box::pin(async move { Ok(stored) })
    }

    fn open<'a>(&'a self, user_id: &'a str) -> PendingFuture<'a, Vec<Action>> {
        let now = OffsetDateTime::now_utc();
        let mut open = newest(
            self.with(|rows| rows.values().filter(|r| r.open(user_id, now)).map(|r| r.action.clone()).collect()),
        );
        open.truncate(OPEN_LIMIT + 1);
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

    fn erase<'a>(&'a self, user_ids: &'a [String]) -> PendingFuture<'a, ()> {
        self.with(|rows| rows.retain(|_, r| !user_ids.contains(&r.action.user_id)));
        Box::pin(async { Ok(()) })
    }
}

fn newest(mut actions: Vec<Action>) -> Vec<Action> {
    actions.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.id.cmp(&b.id)));
    actions
}
