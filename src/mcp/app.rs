use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;

use super::auth::Verifier;
use super::confirmations::{ConfirmConfig, Confirmations, Kind};
use super::options::{Options, Scopes};
use super::pending::Pending;
use super::pending_page::PENDING_PATH;
use super::tool::Tool;

/// The tool calls per person and minute when [`AppConfig::per_minute`] is zero, with a burst of
/// as many, as Go `mcp.DefaultPerMinute`.
pub const DEFAULT_PER_MINUTE: u32 = 120;

/// What an app's `main` hands its MCP module: the deployment side of the server, as Go
/// `mcp.AppConfig`.
#[derive(Clone)]
pub struct AppConfig {
    /// The canonical URI people paste into their MCP client, such as
    /// `https://docs.facile.studio/mcp`. Registre binds tokens to it.
    pub resource: String,
    /// Registre, the only authorization server.
    pub issuer: String,
    /// The server version reported to clients.
    pub version: String,
    /// The browser origins allowed besides the resource's own.
    pub origins: Vec<String>,
    /// Signs the confirmation state handed to clients; derive it with [`super::state_key`].
    pub state_key: Vec<u8>,
    /// Checks access tokens, normally porte's `mcpauth` verifier.
    pub verifier: Arc<dyn Verifier>,
    /// Stores the actions waiting for a person, normally [`super::PostgresPending`]. Required
    /// when the spec has kinds.
    pub pending: Option<Arc<dyn Pending>>,
    /// Caps tool calls per person, with a burst of as many. Zero means [`DEFAULT_PER_MINUTE`].
    pub per_minute: u32,
}

impl AppConfig {
    /// A config for the server at `resource`, with tokens from `issuer` checked by `verifier`.
    pub fn new(resource: impl Into<String>, issuer: impl Into<String>, verifier: Arc<dyn Verifier>) -> Self {
        Self {
            resource: resource.into(),
            issuer: issuer.into(),
            version: String::new(),
            origins: Vec::new(),
            state_key: Vec::new(),
            verifier,
            pending: None,
            per_minute: 0,
        }
    }
}

/// What the app's MCP module itself declares, as Go `mcp.AppSpec`.
#[derive(Clone, Default)]
pub struct AppSpec {
    /// The app, as [`Options::name`].
    pub name: String,
    /// The human name of the app, as [`Options::title`].
    pub title: String,
    /// What a client shows its model, as [`Options::instructions`].
    pub instructions: String,
    /// The scopes, as [`Options::scopes`]. Empty means `<name>.read` and `<name>.write`; a
    /// read-only server sets `read` alone.
    pub scopes: Scopes,
    /// The sensitive actions. None means no confirmations.
    pub kinds: HashMap<String, Kind>,
    /// The curated tools.
    pub tools: Vec<Tool>,
}

/// One app's MCP server options and the confirmations behind its confirm page, as Go `mcp.App`.
/// Tools that ask before acting close over [`Self::confirmations`], so an app builds them after
/// [`App::new`], pushes them onto `options.tools`, then calls [`App::mount`].
#[derive(Clone)]
pub struct App {
    /// The checked server options, ready for [`super::mount`].
    pub options: Options,
    /// Asks before sensitive actions, with its confirm page at `url` + [`PENDING_PATH`]. `None`
    /// when the spec has no kinds.
    pub confirmations: Option<Confirmations>,
    /// The resource's origin, such as `https://docs.facile.studio`.
    pub url: String,
}

impl App {
    /// Builds the options and the confirmations from `cfg` and `spec`, with the suite defaults:
    /// [`DEFAULT_PER_MINUTE`] per person and the confirm page at [`PENDING_PATH`].
    pub fn new(cfg: AppConfig, spec: AppSpec) -> Result<Self, String> {
        let url = origin(&cfg.resource).ok_or("mcp: the resource must be an absolute URI")?;
        let confirmations = if spec.kinds.is_empty() {
            None
        } else {
            let store = cfg.pending.clone().ok_or("mcp: confirmations need a pending store")?;
            Some(Confirmations::new(ConfirmConfig {
                store,
                key: cfg.state_key.clone(),
                confirm_url: format!("{url}{PENDING_PATH}"),
                kinds: spec.kinds,
                pending_ttl: Duration::ZERO,
                ask_ttl: Duration::ZERO,
            })?)
        };
        let per_minute = if cfg.per_minute == 0 { DEFAULT_PER_MINUTE } else { cfg.per_minute };
        let mut options = Options::new(spec.name, cfg.resource, cfg.issuer, cfg.verifier);
        options.title = spec.title;
        options.version = cfg.version;
        options.instructions = spec.instructions;
        options.scopes = spec.scopes;
        options.origins = cfg.origins;
        options.tools = spec.tools;
        options.rate_limit.limit = f64::from(per_minute) / 60.0;
        options.rate_limit.burst = per_minute;
        Ok(Self { options, confirmations, url })
    }

    /// Mounts the server and its metadata on `router`, checking every tool.
    pub fn mount<S: Clone + Send + Sync + 'static>(self, router: Router<S>) -> Result<Router<S>, String> {
        super::mount(router, self.options)
    }
}

fn origin(resource: &str) -> Option<String> {
    let uri: http::Uri = resource.parse().ok()?;
    let scheme = uri.scheme_str().filter(|s| !s.is_empty())?;
    let authority = uri.authority()?.as_str();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    (!host.is_empty()).then(|| format!("{scheme}://{host}"))
}
