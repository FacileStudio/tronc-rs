use std::time::Duration;

const PLAN_FALLBACK: &str = "This needs a plan with AI access.";
const REFUSED_FALLBACK: &str = "Bulle refused the request.";

/// Bulle's 402: the user's plan has no AI access.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanRequired {
    /// The message for the user.
    pub message: String,
    /// Where the user upgrades: empty unless it is a same-origin path or an `https` URL.
    pub upgrade_url: String,
    /// The entitlement the answer names, such as `ai.access`, when it names one.
    pub key: String,
    /// The plan's limit for that entitlement, when the answer gives one.
    pub limit: i64,
}

/// Every way a call to Bulle fails.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Bulle answered 402.
    #[error("bulle: plan required: {}", .0.message)]
    PlanRequired(PlanRequired),
    /// Bulle answered any other 4xx, or a 503 that names a `Retry-After`. 429 is the daily cap,
    /// 409 a batch already running.
    #[error("bulle: answered {status} {code}: {message}")]
    Status {
        /// The HTTP status.
        status: u16,
        /// Bulle's error code.
        code: String,
        /// Bulle's error message.
        message: String,
        /// The wait the `Retry-After` header asks for on a 429 or a 503, delay seconds or an
        /// HTTP date, capped at a day. Zero when it names none, an unreadable value or a past date.
        retry_after: Duration,
    },
    /// No answer from Bulle: a network error, a timeout, a 5xx or an unreadable body.
    #[error("bulle: unreachable: {0}")]
    Unreachable(String),
    /// A path segment was empty, a dot segment, or longer than 200 bytes. Nothing was sent.
    #[error("bulle: invalid path segment")]
    BadSegment,
    /// [`super::Client::decide_ai`] was called without the email of the user it acts for.
    /// Nothing was sent.
    #[error("bulle: decide_ai needs the email of the user it acts for")]
    NoActingFor,
    /// The token source failed. Its error is passed on as it is.
    #[error(transparent)]
    Token(crate::Error),
}

impl Error {
    /// The HTTP status of a 4xx answer, 402 included, or of a 503 that names a `Retry-After`.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::PlanRequired(_) => Some(402),
            Self::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Reports whether Bulle gave no usable answer. A 503 that names a `Retry-After` counts.
    pub fn is_unreachable(&self) -> bool {
        matches!(self, Self::Unreachable(_) | Self::Status { status: 500.., .. })
    }

    /// The wait Bulle's `Retry-After` header asked for, zero when it named none.
    pub fn retry_after(&self) -> Duration {
        match self {
            Self::Status { retry_after, .. } => *retry_after,
            _ => Duration::ZERO,
        }
    }
}

/// The suite envelope for a Bulle failure, as Go `tronc/bulle` answers an app's frontend. A 401
/// from Bulle is about the app's token, not the user's session, so it becomes 503, never 401.
impl From<Error> for crate::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::PlanRequired(plan) => crate::Error::payment_required(or(plan.message, PLAN_FALLBACK))
                .with_upgrade_url(plan.upgrade_url)
                .with_key(plan.key),
            Error::BadSegment => crate::Error::invalid("invalid id"),
            Error::NoActingFor => crate::Error::internal("bulle call failed", "decide_ai without acting_for"),
            Error::Token(inner) => inner,
            Error::Status { status, message, .. } => refusal(status, or(message, REFUSED_FALLBACK)),
            Error::Unreachable(_) => unreachable(),
        }
    }
}

fn refusal(status: u16, message: String) -> crate::Error {
    match status {
        400 => crate::Error::invalid(message),
        403 => crate::Error::forbidden(message),
        404 => crate::Error::not_found(message),
        409 => crate::Error::conflict(message),
        429 => crate::Error::rate_limited(message),
        _ => unreachable(),
    }
}

fn unreachable() -> crate::Error {
    crate::Error::unavailable("Bulle is not reachable right now.")
}

fn or(value: String, fallback: &str) -> String {
    if value.is_empty() { fallback.to_owned() } else { value }
}
