use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use time::OffsetDateTime;

use super::audit::{Event, audit};
use super::call::Caller;
use super::confirmations::{Confirmations, Kind, STATE_CHANGED, STATE_MISSING, STATE_READY, changed, is_changed};
use super::pending::{Action, OUTCOME_DISMISSED, OUTCOME_DONE, OUTCOME_STALE};
use super::signer::invalid_confirmation;
use crate::errors::Result;

impl Confirmations {
    /// Closes one of `who`'s actions without running it.
    pub async fn dismiss(&self, who: &Caller, id: &str) -> Result<()> {
        self.cfg.store.resolve(&who.user_id, id, OUTCOME_DISMISSED).await?;
        self.audit("pending", OUTCOME_DISMISSED, who, id);
        Ok(())
    }

    pub(super) fn audit(&self, tool: &str, outcome: &str, caller: &Caller, target: &str) {
        audit(Event { tool, outcome, caller, target });
    }

    /// Tells a confirm page whether an action still matches what the person was asked about:
    /// [`STATE_READY`], [`STATE_CHANGED`] or [`STATE_MISSING`].
    pub async fn state(&self, action: &Action) -> &'static str {
        let Some(runner) = self.cfg.kinds.get(&action.kind) else {
            return STATE_MISSING;
        };
        match (runner.fingerprint)(action.clone()).await {
            Err(_) => STATE_MISSING,
            Ok(digest) if digest != action.digest => STATE_CHANGED,
            Ok(_) => STATE_READY,
        }
    }

    /// The confirm page URL for action `id`.
    pub fn link(&self, id: &str) -> String {
        let base = self.cfg.confirm_url.as_str();
        let separator = if base.contains('?') { '&' } else { '?' };
        format!("{base}{separator}action={id}")
    }

    pub(super) fn blank(&self, who: &Caller, kind: &str, target: &str, question: &str) -> Action {
        let mut token = [0u8; 18];
        getrandom::fill(&mut token).expect("the operating system random source failed");
        let created_at = OffsetDateTime::now_utc();
        Action {
            id: URL_SAFE_NO_PAD.encode(token),
            user_id: who.user_id.clone(),
            kind: kind.to_owned(),
            target: target.to_owned(),
            digest: String::new(),
            summary: question.to_owned(),
            client_id: who.client_id.clone(),
            created_at,
            expires_at: created_at + self.cfg.pending_ttl,
        }
    }

    pub(super) async fn run(&self, who: &Caller, id: &str, kind: Option<&str>) -> Result<Action> {
        let store = &self.cfg.store;
        let action = store.claim(&who.user_id, id).await?;
        let runner = self.cfg.kinds.get(&action.kind).filter(|_| kind.is_none_or(|k| k == action.kind));
        let Some(runner) = runner else {
            let _ = store.release(id).await;
            return Err(invalid_confirmation());
        };
        let who = Caller { client_id: action.client_id.clone(), ..who.clone() };
        let result = execute(runner, &action).await;
        let outcome = match &result {
            Ok(()) => OUTCOME_DONE,
            Err(error) if is_changed(error) => OUTCOME_STALE,
            Err(_) => "failed",
        };
        let _ = if result.is_err() && outcome != OUTCOME_STALE {
            store.release(id).await
        } else {
            store.finish(id, outcome).await
        };
        self.audit(&runner.tool, outcome, &who, &action.target);
        result.map(|()| action)
    }
}

async fn execute(runner: &Kind, action: &Action) -> Result<()> {
    let digest = (runner.fingerprint)(action.clone()).await?;
    if digest != action.digest {
        return Err(changed());
    }
    (runner.run)(action.clone()).await
}
