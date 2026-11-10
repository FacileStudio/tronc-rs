use crate::errors::{Code, Error};

/// The text a model gets for a failure it cannot fix.
pub const UNAVAILABLE: &str = "the app could not finish this request. Retry once; if it fails again, tell the user";

/// A tool failure as the model reads it: sent as an `isError` result with this text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(pub String);

impl ToolError {
    /// A failure with `message` as its text, written for the model.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl From<Error> for ToolError {
    fn from(error: Error) -> Self {
        failure(error, "")
    }
}

/// Turns a service error into one a model can act on. A bad argument or a missing precondition
/// keeps its message. A missing record keeps its message plus `hint`, which should name the tool
/// that lists valid ids. A plan refusal keeps its message and names the upgrade page. Anything
/// else becomes [`UNAVAILABLE`], so internal detail stays out of the result.
pub fn failure(error: Error, hint: &str) -> ToolError {
    let message = sentence(&error.message);
    ToolError(match error.code {
        Code::NotFound => format!("{message}. {hint}").trim().to_owned(),
        Code::InvalidArgument
        | Code::FailedPrecondition
        | Code::AlreadyExists
        | Code::PermissionDenied
        | Code::RateLimited
        | Code::ResourceExhausted => message,
        Code::PaymentRequired => upgrade(&message, error.upgrade_url.as_deref()),
        _ => UNAVAILABLE.to_owned(),
    })
}

fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

fn upgrade(message: &str, url: Option<&str>) -> String {
    let message = message.strip_suffix('.').unwrap_or(message);
    match url {
        Some(url) if !url.is_empty() => format!("{message}. Upgrade at {url}"),
        _ => format!("{message}."),
    }
}
