use std::time::Duration;

use rmcp::schemars::JsonSchema;
use serde::Serialize;

use super::call::Call;
use super::confirmations::{Confirmations, Outcome};
use super::failure::{ToolError, failure};
use super::tool::Answer;

/// The standard text for the model once the person declined a sensitive action, as Go
/// `mcp.MessageDeclined`.
pub const MESSAGE_DECLINED: &str = "The user declined. Nothing happened.";

/// The hint added to a failed resume, such as an action already confirmed elsewhere.
pub const ASK_AGAIN: &str = "Call the tool again to ask the user afresh.";

/// What a sensitive tool answers, from [`Confirmations::ask`] and [`Confirmations::answered`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct Confirmation {
    /// `pending_confirmation`, `declined` or `done`.
    pub status: String,
    /// The link the person opens to confirm, when the client could not ask.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub confirm_url: String,
    /// What happened and what to do next, for the model.
    pub message: String,
}

impl Confirmation {
    fn from(outcome: Outcome) -> Self {
        Self { status: outcome.status.to_owned(), confirm_url: outcome.confirm_url, message: outcome.message }
    }
}

fn lifetime(ttl: Duration) -> String {
    let plural = |n: u64, unit: &str| if n == 1 { format!("1 {unit}") } else { format!("{n} {unit}s") };
    match ttl.as_secs() / 86_400 {
        0 => plural((ttl.as_secs() / 3600).max(1), "hour"),
        days => plural(days, "day"),
    }
}

pub(super) fn waiting(ttl: Duration) -> String {
    format!(
        "Nothing happened yet: your client cannot ask the user to confirm. Give the user confirm_url and ask them \
         to confirm there. Do not call this tool again for the same target; the request stays open for {}.",
        lifetime(ttl)
    )
}

impl Confirmations {
    /// The standard text for the model when a sensitive action waits on the confirm page, as Go
    /// `Confirmations.Waiting`: nothing ran, hand over `confirm_url`, do not ask again. A request
    /// that reuses an open action counts down from that action's expiry instead.
    pub fn waiting(&self) -> String {
        waiting(self.cfg.pending_ttl)
    }

    /// Asks the person `question` before an action of `kind` runs on `target`, once the tool made
    /// its own checks: an `input_required` result when the client can elicit, otherwise a
    /// [`Confirmation`] holding the confirm link and [`Self::waiting`]. A service error comes
    /// back through [`failure`].
    pub async fn ask(
        &self,
        call: &Call,
        kind: &str,
        target: &str,
        question: &str,
    ) -> Result<Answer<Confirmation>, ToolError> {
        let (asked, outcome) = self.request(call, kind, target, question).await.map_err(|e| failure(e, ""))?;
        Ok(match asked {
            Some(question) => Answer::InputRequired(question),
            None => Answer::Output(Confirmation::from(outcome)),
        })
    }

    /// Handles the retry that carries the person's answer, which a sensitive tool checks first
    /// with [`Call::retrying`]: a decline answers [`MESSAGE_DECLINED`], an accept runs the action
    /// and answers `done`, such as "The user confirmed and the board was deleted."
    pub async fn answered(&self, call: &Call, kind: &str, done: &str) -> Result<Confirmation, ToolError> {
        let outcome = self.resume(call, kind).await.map_err(|e| failure(e, ASK_AGAIN))?;
        let mut answer = Confirmation::from(outcome);
        if answer.message.is_empty() {
            answer.message = done.to_owned();
        }
        Ok(answer)
    }
}
