use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::routing::{get, post};
use http::request::Parts;
use serde::Serialize;
use time::OffsetDateTime;

use super::call::Caller;
use super::confirmations::{Confirmations, Outcome};
use super::pending::Action;
use super::pending_routes as routes;
use crate::errors::{Error, Result};

/// The path of the confirm page on the app's origin; [`super::App`] puts it in confirm links.
pub const PENDING_PATH: &str = "/pending";

/// Where [`PendingPage::routes`] serves the confirm page's API, as Go `mcp.PendingAPIPath`. The
/// app nests the routes under `/api`, behind its own session middleware.
pub const PENDING_API_PATH: &str = "/mcp/pending";

/// The confirm page routes, as `(method, path, summary, success status)` rows for the app's API
/// reference, as Go `mcp.PendingRoutes`.
pub const PENDING_ROUTES: [(&str, &str, &str, &str); 3] = [
    ("GET", "/mcp/pending", "List actions waiting for confirmation", "200"),
    ("POST", "/mcp/pending/{id}/confirm", "Confirm an action", "200"),
    ("POST", "/mcp/pending/{id}/dismiss", "Dismiss an action", "200"),
];

/// The refusal for a confirm page request that does not come from the person's own session:
/// an agent never confirms its own request.
pub const DELEGATED: &str = "only the person's own session can confirm actions";

/// The `permission_denied` error carrying [`DELEGATED`], as Go `mcp.ErrDelegated`.
pub fn delegated() -> Error {
    Error::forbidden(DELEGATED)
}

/// What every confirm page entry shows, as Go `mcp.PendingItem`: the action, the client that
/// asked and whether the target still matches the question. An app's own item flattens it next
/// to what the action targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingItem {
    /// The action id, from the confirm link.
    pub id: String,
    /// The kind of action.
    pub kind: String,
    /// The OAuth client that asked.
    pub client_id: String,
    /// The question the person was asked.
    pub summary: String,
    /// When the action was opened.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When the action stops waiting.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    /// `ready`, `changed` or `missing`.
    pub state: String,
}

/// The body of `GET /mcp/pending`: the person's open actions, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingList<T> {
    /// The actions.
    pub actions: Vec<T>,
    /// More actions wait than the page lists; the oldest are left out.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// The body answered when an action is confirmed or dismissed. `url` is set when the action made
/// something to open, such as a public link.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PendingResult {
    /// `done` or `dismissed`.
    pub status: String,
    /// What the action made, when there is something to open.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub url: String,
}

/// A boxed future the confirm page hooks return.
pub type PageFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Returns the signed-in person's user id. It must refuse with [`delegated`], or another error,
/// for anything but the person's own browser session.
pub type UserFn = Arc<dyn Fn(Parts) -> PageFuture<Result<String>> + Send + Sync>;

/// Adds what the action targets, as it is now, to the item.
pub type DescribeFn<T> = Arc<dyn Fn(PendingItem, Action) -> PageFuture<T> + Send + Sync>;

/// The body answered after a confirmed run.
pub type DoneFn = Arc<dyn Fn(Outcome) -> PageFuture<PendingResult> + Send + Sync>;

/// Serves the API of an app's confirm page, as Go `mcp.PendingPage`: the list of open actions,
/// confirm and dismiss. `T` is the app's item, [`PendingItem`] unless [`Self::describe`] sets
/// one.
pub struct PendingPage<T = PendingItem> {
    confirmations: Option<Confirmations>,
    actor: String,
    user: UserFn,
    describe: DescribeFn<T>,
    pub(super) done: Option<DoneFn>,
}

impl PendingPage<PendingItem> {
    /// A page over `confirmations`, `None` when MCP is off, which answers `404`. `actor` marks a
    /// confirmation made on the page in the audit, such as `docs-web`. `user` reads the person
    /// from the request head, which the app's session middleware filled.
    pub fn new<F, Fut>(confirmations: Option<Confirmations>, actor: impl Into<String>, user: F) -> Self
    where
        F: Fn(Parts) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<String>> + Send + 'static,
    {
        Self {
            confirmations,
            actor: actor.into(),
            user: Arc::new(move |parts| Box::pin(user(parts))),
            describe: Arc::new(|item, _| Box::pin(std::future::ready(item))),
            done: None,
        }
    }
}

impl<T: Serialize + Send + 'static> PendingPage<T> {
    /// Lists items of type `U`, built from each entry and its action.
    pub fn describe<U, F, Fut>(self, describe: F) -> PendingPage<U>
    where
        F: Fn(PendingItem, Action) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = U> + Send + 'static,
    {
        PendingPage {
            confirmations: self.confirmations,
            actor: self.actor,
            user: self.user,
            describe: Arc::new(move |item, action| Box::pin(describe(item, action))),
            done: self.done,
        }
    }

    /// Answers `done(outcome)` after a confirmed run, in place of `{"status":"done"}`.
    pub fn done<F, Fut>(mut self, done: F) -> Self
    where
        F: Fn(Outcome) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = PendingResult> + Send + 'static,
    {
        self.done = Some(Arc::new(move |outcome| Box::pin(done(outcome))));
        self
    }

    /// `GET /mcp/pending`, `POST /mcp/pending/{id}/confirm` and `POST /mcp/pending/{id}/dismiss`,
    /// for the app to nest under `/api` behind its session middleware.
    pub fn routes<S: Clone + Send + Sync + 'static>(self) -> Router<S> {
        Router::new()
            .route(PENDING_API_PATH, get(routes::list::<T>))
            .route("/mcp/pending/{id}/confirm", post(routes::confirm::<T>))
            .route("/mcp/pending/{id}/dismiss", post(routes::dismiss::<T>))
            .with_state(Arc::new(self))
    }

    /// Renders `user_id`'s open actions with their targets as they are now.
    pub async fn list(&self, user_id: &str) -> Result<PendingList<T>> {
        let confirmations = self.confirmations()?;
        let (open, truncated) = confirmations.list(user_id).await?;
        let mut actions = Vec::with_capacity(open.len());
        for action in open {
            let item = PendingItem {
                id: action.id.clone(),
                kind: action.kind.clone(),
                client_id: action.client_id.clone(),
                summary: action.summary.clone(),
                created_at: action.created_at,
                expires_at: action.expires_at,
                state: confirmations.state(&action).await.to_owned(),
            };
            actions.push((self.describe)(item, action).await);
        }
        Ok(PendingList { actions, truncated })
    }

    fn confirmations(&self) -> Result<&Confirmations> {
        self.confirmations.as_ref().ok_or_else(|| Error::not_found("MCP is off"))
    }

    pub(super) async fn caller(&self, request: Request) -> Result<(&Confirmations, Caller)> {
        let confirmations = self.confirmations()?;
        let user_id = (self.user)(request.into_parts().0).await?;
        if user_id.is_empty() {
            return Err(Error::unauthorized("missing auth"));
        }
        Ok((confirmations, Caller { user_id, actor: self.actor.clone(), ..Caller::default() }))
    }
}
