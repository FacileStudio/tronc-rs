use std::sync::Arc;

use axum::Router;
use axum::extract::Path;
use axum::routing::{get, post};
use http::request::Parts;
use serde::{Deserialize, Serialize};

use super::client::Client;
use super::request::TokenSource;
use super::types::IdeaList;
use crate::json::Json;

const PLAN_MESSAGE: &str = "Ideas need a plan with AI access.";

/// Builds the token source for the signed-in user of a request, usually from what the app's
/// auth layer put in the request extensions.
pub type TokensFn = Arc<dyn Fn(&Parts) -> Box<dyn TokenSource> + Send + Sync>;

/// Reads the request's active space: the Registre shared space id, or `""` for the personal
/// space.
pub type SpaceFn = Arc<dyn Fn(&Parts) -> String + Send + Sync>;

/// Reports whether the request's plan includes `ai.access`.
pub type AiAccessFn = Arc<dyn Fn(&Parts) -> bool + Send + Sync>;

/// Configures [`mount_ideas`].
#[derive(Clone)]
pub struct IdeasOptions {
    /// The app's name in Bulle's idea enum, such as `agenda` or `toile`.
    pub app: String,
    /// The client the routes call Bulle with.
    pub client: Client,
    /// The token source of each request's user.
    pub tokens: TokensFn,
    /// The request's active space. `None` means always personal.
    pub space: Option<SpaceFn>,
    /// The plan check. When it says no, the routes that spend AI answer 402 without calling
    /// Bulle and the list answers `ai_access: false`. `None` leaves the check to Bulle.
    pub ai_access: Option<AiAccessFn>,
    /// The upgrade link sent with that 402, such as Registre's `/spaces` page.
    pub upgrade_url: String,
}

impl IdeasOptions {
    /// Options for `app`, calling Bulle through `client` with each request's `tokens`.
    pub fn new(app: impl Into<String>, client: Client, tokens: TokensFn) -> Self {
        Self { app: app.into(), client, tokens, space: None, ai_access: None, upgrade_url: String::new() }
    }
}

/// `GET /ideas`: the plan state and, with AI access, the ideas.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct IdeasPage {
    /// Whether the plan includes AI.
    pub ai_access: bool,
    /// Where to upgrade, sent only without AI access.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub upgrade_url: String,
    /// The ideas, empty without AI access.
    #[serde(flatten)]
    pub list: IdeaList,
}

#[derive(Deserialize)]
struct Decision {
    approve: bool,
}

pub(super) struct Ideas {
    pub(super) options: IdeasOptions,
}

/// Serves the app's own frontend under `/ideas` on `router`, the router the app's API lives on
/// (so the routes are `/api/ideas...`), as Go `bulle.MountIdeas`:
///
/// - `GET /ideas`: the page, `ai_access`, `ideas`, `generating`
/// - `POST /ideas/generate`: a new batch, 202 (402 without AI access)
/// - `POST /ideas/{id}/run`: run an idea in the background, 202 (402 without AI access)
/// - `POST /ideas/{id}/dismiss`: hide an idea, 204
/// - `GET /ideas/runs/{run}`: a run's state, outputs and approvals
/// - `POST /ideas/runs/{run}/approvals/{call}`: `{"approve": bool}`, 204; approving needs AI access
///
/// Mount it behind the app's authentication, so [`IdeasOptions::tokens`] finds the user. Bulle
/// failures answer the suite envelope through `From<bulle::Error> for tronc::Error`.
pub fn mount_ideas<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    options: IdeasOptions,
) -> Result<Router<S>, String> {
    if !app_name(&options.app) {
        return Err("bulle: app must be the app's name in Bulle, such as agenda".to_owned());
    }
    let h = Arc::new(Ideas { options });
    let (list, generate, run, dismiss, status, decide) = (h.clone(), h.clone(), h.clone(), h.clone(), h.clone(), h);
    let routes = Router::new()
        .route("/ideas", get(move |parts: Parts| async move { list.list(parts).await }))
        .route("/ideas/generate", post(move |parts: Parts| async move { generate.generate(parts).await }))
        .route(
            "/ideas/{id}/run",
            post(move |parts: Parts, Path(id): Path<String>| async move { run.run(parts, id).await }),
        )
        .route(
            "/ideas/{id}/dismiss",
            post(move |parts: Parts, Path(id): Path<String>| async move { dismiss.dismiss(parts, id).await }),
        )
        .route(
            "/ideas/runs/{run}",
            get(move |parts: Parts, Path(id): Path<String>| async move { status.status(parts, id).await }),
        )
        .route(
            "/ideas/runs/{run}/approvals/{call}",
            post(move |parts: Parts, Path(ids): Path<(String, String)>, Json(body): Json<Decision>| async move {
                decide.decide(parts, ids, body.approve).await
            }),
        );
    Ok(router.merge(routes))
}

fn app_name(app: &str) -> bool {
    let mut chars = app.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && app.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

impl Ideas {
    pub(super) fn space(&self, parts: &Parts) -> String {
        self.options.space.as_ref().map(|space| space(parts)).unwrap_or_default()
    }

    pub(super) fn ai_access(&self, parts: &Parts) -> bool {
        self.options.ai_access.as_ref().is_none_or(|check| check(parts))
    }

    pub(super) fn require_ai(&self, parts: &Parts) -> Result<(), crate::Error> {
        if self.ai_access(parts) {
            return Ok(());
        }
        Err(crate::Error::payment_required(PLAN_MESSAGE).with_upgrade_url(self.options.upgrade_url.clone()))
    }
}
