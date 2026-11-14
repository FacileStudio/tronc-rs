use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::InputRequiredResult;

use super::call::{Call, Caller};
use super::pending::{Action, OUTCOME_DECLINED, Pending};
use super::signer::{Signer, Verdict};
use crate::errors::{Error, Result};

/// A sensitive action waits for the person; the result carries `confirm_url` when the client
/// could not ask.
pub const STATUS_PENDING: &str = "pending_confirmation";
/// The person declined; nothing ran.
pub const STATUS_DECLINED: &str = "declined";
/// The action ran.
pub const STATUS_DONE: &str = "done";

/// On a confirm page: the target is as the person was asked about it.
pub const STATE_READY: &str = "ready";
/// On a confirm page: the target changed since the question.
pub const STATE_CHANGED: &str = "changed";
/// On a confirm page: the target or its kind is gone.
pub const STATE_MISSING: &str = "missing";

/// How long a pending action waits when [`ConfirmConfig::pending_ttl`] is zero.
pub const DEFAULT_PENDING_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// How long a signed question stays valid when [`ConfirmConfig::ask_ttl`] is zero.
pub const DEFAULT_ASK_TTL: Duration = Duration::from_secs(15 * 60);

/// The refusal for a target that changed after the person was asked: they confirmed other
/// content, so the action closes.
pub const CHANGED: &str =
    "this changed after the user was asked, so nothing happened. Call the tool again to ask about the current version";

/// The `failed_precondition` error carrying [`CHANGED`].
pub fn changed() -> Error {
    Error::failed(CHANGED)
}

/// Reports whether `error` is [`changed`].
pub fn is_changed(error: &Error) -> bool {
    error.code == crate::Code::FailedPrecondition && error.message == CHANGED
}

/// The future a [`Kind`] callback returns.
pub type KindFuture<T> = Pin<Box<dyn Future<Output = Result<T>> + Send>>;

/// Runs one kind of sensitive action.
#[derive(Clone)]
pub struct Kind {
    /// The tool that asks for this kind, for the audit.
    pub tool: String,
    /// A digest of the target as it is now. It is stored when asking and compared before
    /// running, so an edit in between closes the action. A target that is gone answers its
    /// `not_found` error.
    pub fingerprint: Arc<dyn Fn(Action) -> KindFuture<String> + Send + Sync>,
    /// Performs the action.
    pub run: Arc<dyn Fn(Action) -> KindFuture<()> + Send + Sync>,
}

impl Kind {
    /// A kind asked for by `tool`, from two async callbacks.
    pub fn new<F, FF, R, RF>(tool: impl Into<String>, fingerprint: F, run: R) -> Self
    where
        F: Fn(Action) -> FF + Send + Sync + 'static,
        FF: Future<Output = Result<String>> + Send + 'static,
        R: Fn(Action) -> RF + Send + Sync + 'static,
        RF: Future<Output = Result<()>> + Send + 'static,
    {
        Self {
            tool: tool.into(),
            fingerprint: Arc::new(move |action| Box::pin(fingerprint(action))),
            run: Arc::new(move |action| Box::pin(run(action))),
        }
    }
}

/// Describes the confirmations of one app.
#[derive(Clone)]
pub struct ConfirmConfig {
    /// Keeps pending actions; [`super::PostgresPending`] in production.
    pub store: Arc<dyn Pending>,
    /// Signs the question state. Secret, and stable across restarts and replicas.
    pub key: Vec<u8>,
    /// The app page where the person confirms, such as `https://agenda.facile.studio/pending`.
    /// The action id is added as the `action` query parameter.
    pub confirm_url: String,
    /// Maps each kind to how it runs.
    pub kinds: HashMap<String, Kind>,
    /// How long an action waits. Zero means [`DEFAULT_PENDING_TTL`].
    pub pending_ttl: Duration,
    /// How long a signed question stays valid. Zero means [`DEFAULT_ASK_TTL`].
    pub ask_ttl: Duration,
}

/// Where a sensitive action stands after a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// [`STATUS_PENDING`], [`STATUS_DECLINED`] or [`STATUS_DONE`].
    pub status: &'static str,
    /// The action, `None` after a decline.
    pub action: Option<Action>,
    /// The link to give the person when the client could not ask. Empty otherwise.
    pub confirm_url: String,
    /// The standard text for the model with a confirm link ([`Confirmations::waiting`]) or
    /// after a decline ([`super::MESSAGE_DECLINED`]), as Go `Outcome.Message`. Empty otherwise.
    pub message: String,
}

/// Asks a person before a sensitive action runs: through an MRTR `input_required` question
/// when the client can elicit, otherwise through a pending action and a confirm link the client
/// hands over. Nothing runs without the person's answer.
#[derive(Clone)]
pub struct Confirmations {
    pub(super) cfg: ConfirmConfig,
    pub(super) signer: Signer,
}

impl Confirmations {
    /// Checks `cfg` and fills its defaults.
    pub fn new(mut cfg: ConfirmConfig) -> std::result::Result<Self, String> {
        let link: Option<http::Uri> = cfg.confirm_url.parse().ok();
        if cfg.key.is_empty() || cfg.kinds.is_empty() {
            return Err("mcp: confirmations need a store, a key and at least one kind".to_owned());
        }
        if link.is_none_or(|uri| uri.scheme().is_none() || uri.host().is_none_or(str::is_empty)) {
            return Err("mcp: the confirm URL must be absolute".to_owned());
        }
        if let Some(name) = cfg.kinds.iter().find(|(_, kind)| kind.tool.is_empty()).map(|(name, _)| name) {
            return Err(format!("mcp: kind {name} needs a tool, a fingerprint and a run"));
        }
        if cfg.pending_ttl.is_zero() {
            cfg.pending_ttl = DEFAULT_PENDING_TTL;
        }
        if cfg.ask_ttl.is_zero() {
            cfg.ask_ttl = DEFAULT_ASK_TTL;
        }
        let signer = Signer { key: cfg.key.clone(), ttl: cfg.ask_ttl };
        Ok(Self { cfg, signer })
    }

    /// Opens a pending action of `kind` on `target` and asks the person `question`, or reuses
    /// the person's open action of that kind on that target with the same digest, asked by the
    /// same OAuth client, so an agent that loops on a tool opens one action. Another client of
    /// the same person opens its own action. A reused action keeps its first expiry, which the
    /// waiting text counts down from. When the client can elicit, the first value is the
    /// `input_required` result for the tool to return as is. Otherwise it is `None` and the
    /// outcome holds [`STATUS_PENDING`], the confirm link and [`Self::waiting`]. Either way
    /// nothing has run yet.
    pub async fn request(
        &self,
        call: &Call,
        kind: &str,
        target: &str,
        question: &str,
    ) -> Result<(Option<InputRequiredResult>, Outcome)> {
        let who = &call.caller;
        let runner = self.cfg.kinds.get(kind).ok_or_else(|| Error::internal("mcp: unknown confirmation kind", kind))?;
        let mut action = self.blank(who, kind, target, question);
        action.digest = (runner.fingerprint)(action.clone()).await?;
        let action = self.cfg.store.create(&action).await?;
        self.audit(&runner.tool, "requested", who, target);
        if call.can_elicit() {
            let question = self.signer.ask(&who.user_id, &action.id, question);
            return Ok((
                Some(question),
                Outcome {
                    status: STATUS_PENDING,
                    action: Some(action),
                    confirm_url: String::new(),
                    message: String::new(),
                },
            ));
        }
        let confirm_url = self.link(&action.id);
        let left = (action.expires_at - time::OffsetDateTime::now_utc()).whole_seconds().max(0) as u64;
        let message = super::ask::waiting(Duration::from_secs((left + 1800) / 3600 * 3600));
        Ok((None, Outcome { status: STATUS_PENDING, action: Some(action), confirm_url, message }))
    }

    /// Handles the retry that carries the person's answer to a question about `kind`: a decline
    /// closes the action, an accept runs it once.
    pub async fn resume(&self, call: &Call, kind: &str) -> Result<Outcome> {
        let who = &call.caller;
        let (id, verdict) = self.signer.answer(call)?;
        if verdict == Verdict::Declined {
            self.cfg.store.resolve(&who.user_id, &id, OUTCOME_DECLINED).await?;
            let tool = self.cfg.kinds.get(kind).map_or("", |k| k.tool.as_str());
            self.audit(tool, OUTCOME_DECLINED, who, "");
            let message = super::MESSAGE_DECLINED.to_owned();
            return Ok(Outcome { status: STATUS_DECLINED, action: None, confirm_url: String::new(), message });
        }
        let action = self.run(who, &id, Some(kind)).await?;
        Ok(Outcome { status: STATUS_DONE, action: Some(action), confirm_url: String::new(), message: String::new() })
    }

    /// Runs one of `who`'s actions from the confirm page. The page must refuse a delegated token
    /// first: an agent never confirms its own request.
    pub async fn confirm(&self, who: &Caller, id: &str) -> Result<Outcome> {
        let action = self.run(who, id, None).await?;
        Ok(Outcome { status: STATUS_DONE, action: Some(action), confirm_url: String::new(), message: String::new() })
    }
}
