use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

/// A run that is still working.
pub const STATE_RUNNING: &str = "running";
/// A run holding a call until the user decides.
pub const STATE_NEEDS_APPROVAL: &str = "needs_approval";
/// A run that finished.
pub const STATE_DONE: &str = "done";
/// A run that errored, was stopped or was interrupted.
pub const STATE_FAILED: &str = "failed";

/// What an idea is based on: a label and Bulle's reference, a fingerprint such as
/// `courrier:5/email:812` or a URL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    /// What the user sees.
    pub label: String,
    /// The fingerprint or URL.
    #[serde(rename = "ref")]
    pub reference: String,
}

/// One idea card.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Idea {
    /// The idea's id.
    pub id: String,
    /// The Bulle space it belongs to.
    pub space_id: String,
    /// The app it is for, such as `agenda`.
    pub app: String,
    /// `mail`, `task`, `event`, `follow_up`, `research` or `other`.
    pub kind: String,
    /// The card title.
    pub title: String,
    /// Why Bulle suggests it.
    pub why: String,
    /// A ready draft, when the kind has one.
    pub draft: String,
    /// Up to five sources.
    #[serde(deserialize_with = "nullable")]
    pub sources: Vec<Source>,
    /// Bulle's status, such as `new`.
    pub status: String,
    /// When Bulle made it.
    #[serde(with = "time::serde::rfc3339::option")]
    pub created_at: Option<OffsetDateTime>,
    /// When it expires.
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
}

/// An app's open ideas in one space.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdeaList {
    /// The open ideas, never `null`.
    #[serde(deserialize_with = "nullable")]
    pub ideas: Vec<Idea>,
    /// Whether a batch is being generated.
    pub generating: bool,
    /// When the last batch ran.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_batch_at: Option<OffsetDateTime>,
}

/// The batch a generate started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Batch {
    /// The batch id.
    pub batch_id: String,
}

/// The background run doing an idea.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RunStart {
    /// The run id, for [`super::Client::run_status`].
    pub run_id: String,
    /// The Bulle session the run writes to.
    pub session_id: String,
    /// The idea it does.
    pub idea_id: String,
}

/// One call of a run that changed the user's data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    /// The tool call id.
    pub call_id: String,
    /// The tool that ran.
    pub tool: String,
    /// The app it touched.
    pub app: String,
    /// The id of what it made or changed.
    pub id: String,
    /// A link to it: empty unless it is an `http` or `https` URL.
    pub url: String,
    /// A short preview.
    pub preview: String,
}

/// A call waiting for the user. Answer it with [`super::Client::decide`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Approval {
    /// The tool call id.
    pub call_id: String,
    /// The tool asking.
    pub tool: String,
    /// Its arguments, as Bulle sent them.
    pub args: Value,
    /// Whether the user may allow the tool for good.
    pub grantable: bool,
}

/// A background run, as [`super::Client::run_status`] reads it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Run {
    /// The run id.
    pub id: String,
    /// The Bulle session.
    pub session_id: String,
    /// Bulle's own status.
    pub status: String,
    /// One of the `STATE_*` constants, derived from `status`.
    pub state: String,
    /// What the run did.
    pub summary: String,
    /// The calls that changed the user's data, never `null`.
    #[serde(deserialize_with = "nullable")]
    pub outputs: Vec<Output>,
    /// The calls waiting for the user, never `null`.
    #[serde(deserialize_with = "nullable")]
    pub pending_approvals: Vec<Approval>,
    /// When it started.
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    /// When it finished.
    #[serde(with = "time::serde::rfc3339::option")]
    pub finished_at: Option<OffsetDateTime>,
    /// Opens the run's chat in Bulle.
    pub continue_url: String,
}

/// Maps Bulle's run status: waiting, or running with a call held, needs approval; finished is
/// done; error, stopped and interrupted failed; anything else is running.
pub fn run_state(status: &str, pending: usize) -> &'static str {
    match status {
        "waiting" => STATE_NEEDS_APPROVAL,
        "running" if pending > 0 => STATE_NEEDS_APPROVAL,
        "finished" => STATE_DONE,
        "error" | "stopped" | "interrupted" => STATE_FAILED,
        _ => STATE_RUNNING,
    }
}

fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(deserializer: D) -> Result<T, D::Error> {
    Ok(Option::deserialize(deserializer)?.unwrap_or_default())
}
