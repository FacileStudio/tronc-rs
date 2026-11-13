use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;

use super::auth::{TokenInfo, Verifier};
use super::tool::Tool;
use crate::http::{RateLimitConfig, remote_addr_key64};

/// How long a client may keep the tool list when [`Options::list_ttl`] is zero.
pub const DEFAULT_LIST_TTL: Duration = Duration::from_secs(600);

const DEFAULT_RATE: f64 = 2.0;
const DEFAULT_BURST: u32 = 30;
const DEFAULT_MAX_KEYS: usize = 10_000;

/// An app's MCP scopes. A token with `write` may call every tool; a token with only `read` may
/// call the read-only ones. `write` is empty on a read-only server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scopes {
    /// The read scope, such as `agenda.read`.
    pub read: String,
    /// The write scope, such as `agenda.write`, or empty.
    pub write: String,
}

impl Scopes {
    /// The scopes, read first, as `scopes_supported` lists them.
    pub fn list(&self) -> Vec<String> {
        if self.write.is_empty() { vec![self.read.clone()] } else { vec![self.read.clone(), self.write.clone()] }
    }
}

/// One app's MCP server.
#[derive(Clone)]
pub struct Options {
    /// The app, such as `agenda`: the server name, the prefix every tool name starts with
    /// (`agenda_`), and the default scope prefix.
    pub name: String,
    /// The human name of the app, such as `Agenda`.
    pub title: String,
    /// The server version reported to clients.
    pub version: String,
    /// What a client shows its model about this server.
    pub instructions: String,
    /// The canonical URI people paste into their MCP client, such as
    /// `https://agenda.facile.studio/mcp`. Tokens are bound to it, and its path is where the
    /// endpoint is served.
    pub resource: String,
    /// The authorization server named in the metadata.
    pub issuer: String,
    /// Checks access tokens.
    pub verifier: Arc<dyn Verifier>,
    /// The read and write scopes. Empty means `<name>.read` and `<name>.write`. A server whose
    /// tools are all read-only may set `read` alone, and then advertises only that scope.
    pub scopes: Scopes,
    /// Caps calls per person. A zero `limit` means two a second with a burst of thirty, and
    /// [`super::App::new`] sets 120 a minute with a burst of 120. No `key` keys on the token's
    /// user, or on the caller's address (its /64 for IPv6) without one. A zero `max_keys` keeps
    /// ten thousand keys.
    pub rate_limit: RateLimitConfig,
    /// Browser origins allowed besides the resource's own. Requests without `Origin`, from native
    /// clients, always pass.
    pub origins: Vec<String>,
    /// The curated tools, built with [`super::tool`].
    pub tools: Vec<Tool>,
    /// How long a client may cache list results. Zero means [`DEFAULT_LIST_TTL`]. The scope is
    /// always private, because results are served under one person's token.
    pub list_ttl: Duration,
}

impl Options {
    /// Options for app `name` at `resource`, with tokens from `issuer` checked by `verifier`.
    pub fn new(
        name: impl Into<String>,
        resource: impl Into<String>,
        issuer: impl Into<String>,
        verifier: Arc<dyn Verifier>,
    ) -> Self {
        Self {
            name: name.into(),
            title: String::new(),
            version: String::new(),
            instructions: String::new(),
            resource: resource.into(),
            issuer: issuer.into(),
            verifier,
            scopes: Scopes::default(),
            rate_limit: RateLimitConfig::default(),
            origins: Vec::new(),
            tools: Vec::new(),
            list_ttl: Duration::ZERO,
        }
    }
}

fn valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase()) && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

pub(super) fn normalize(opts: &mut Options) -> Result<(), String> {
    if !valid_name(&opts.name) {
        return Err(format!("mcp: name {:?} must be lowercase letters and digits", opts.name));
    }
    if opts.issuer.is_empty() {
        return Err("mcp: an issuer is required".to_owned());
    }
    if opts.scopes == Scopes::default() {
        opts.scopes = Scopes { read: format!("{}.read", opts.name), write: format!("{}.write", opts.name) };
    }
    if opts.scopes.read.is_empty() {
        return Err("mcp: a read scope is required".to_owned());
    }
    if opts.list_ttl.is_zero() {
        opts.list_ttl = DEFAULT_LIST_TTL;
    }
    if opts.rate_limit.limit <= 0.0 {
        opts.rate_limit.limit = DEFAULT_RATE;
        opts.rate_limit.burst = opts.rate_limit.burst.max(DEFAULT_BURST);
    }
    if opts.rate_limit.max_keys == 0 {
        opts.rate_limit.max_keys = DEFAULT_MAX_KEYS;
    }
    if opts.rate_limit.key.is_none() {
        opts.rate_limit.key = Some(Arc::new(token_user));
    }
    Ok(())
}

fn token_user(request: &Request) -> String {
    match request.extensions().get::<TokenInfo>().filter(|info| !info.user_id.is_empty()) {
        Some(info) => format!("user:{}", info.user_id),
        None => format!("addr:{}", remote_addr_key64(request)),
    }
}
