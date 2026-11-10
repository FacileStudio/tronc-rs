//! The configuration every Facile API shares, and typed helpers for the rest.
//!
//! `APP_ENV` never gates security behaviour: CORS is decided by the configured origin list
//! alone, so a missing `APP_ENV` cannot open an app up.

mod duration;
mod proxies;
mod typed;
mod vars;

use ipnet::IpNet;

pub use vars::Vars;

/// A configuration variable that is missing or malformed. Displays as `env: <reason>`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("env: {0}")]
pub struct EnvError(pub String);

/// The deployment an app is running in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Environment {
    /// Local development, the default.
    #[default]
    Development,
    /// A staging deployment.
    Staging,
    /// Production.
    Production,
}

impl Environment {
    /// Maps a name onto an environment, defaulting to development.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "production" | "prod" => Self::Production,
            "staging" | "stage" => Self::Staging,
            _ => Self::Development,
        }
    }
}

/// The CORS origin variables, read in order; the first one set wins.
pub const CORS_ORIGIN_KEYS: [&str; 7] = [
    "CORS_ALLOWED_ORIGINS",
    "ALLOWED_ORIGINS",
    "DOMAINS",
    "DOMAIN",
    "CORS_ORIGINS",
    "TRUSTED_ORIGINS",
    "CLIENT_ORIGIN",
];

/// The default listen port, shared with [`crate::healthcheck`] so the probe and the server agree.
pub const DEFAULT_PORT: u16 = 8080;

/// The configuration shared by every API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Core {
    /// `APP_ENV`.
    pub app_env: Environment,
    /// `PORT`, default [`DEFAULT_PORT`].
    pub port: u16,
    /// `LOG_LEVEL`, default `info`.
    pub log_level: String,
    /// `DATABASE_URL`.
    pub database_url: String,
    /// The first of [`CORS_ORIGIN_KEYS`] that is set, split on commas.
    pub cors_allowed_origins: Vec<String>,
    /// `JOURNAL_URL`.
    pub journal_url: String,
    /// `JOURNAL_TOKEN`.
    pub journal_token: String,
    /// The peers whose `X-Forwarded-For` is believed, from `TRUSTED_PROXIES`.
    pub trusted_proxies: Vec<IpNet>,
    /// The CDN edge ranges, set when `TRUSTED_PROXIES` names `cloudflare`.
    pub cdn_proxies: Vec<IpNet>,
    /// The CDN's visitor header, `Cf-Connecting-Ip` when `TRUSTED_PROXIES` names `cloudflare`.
    pub cdn_header: String,
}

impl Core {
    /// Reads the core configuration from the process. `DATABASE_URL` is required.
    pub fn load() -> Result<Self, EnvError> {
        Self::from_vars(&Vars::process(), true)
    }

    /// Reads the core configuration for a service with no database of its own.
    pub fn load_without_database() -> Result<Self, EnvError> {
        Self::from_vars(&Vars::process(), false)
    }

    /// Reads the core configuration from `vars`.
    pub fn from_vars(vars: &Vars, require_database: bool) -> Result<Self, EnvError> {
        let port = vars.int("PORT", i64::from(DEFAULT_PORT))?;
        let port = u16::try_from(port).map_err(|_| EnvError(format!("PORT must be a port number, got {port}")))?;
        let trusted_proxies = proxies::trusted(vars)?;
        let (cdn_proxies, cdn_header) = proxies::cdn(vars);
        let database_url = vars.string("DATABASE_URL", "");
        if require_database && database_url.is_empty() {
            return Err(EnvError("DATABASE_URL is required".into()));
        }
        Ok(Self {
            app_env: Environment::parse(&vars.string("APP_ENV", "development")),
            port,
            log_level: vars.string("LOG_LEVEL", "info"),
            database_url,
            cors_allowed_origins: cors_origins(vars),
            journal_url: vars.string("JOURNAL_URL", ""),
            journal_token: vars.string("JOURNAL_TOKEN", ""),
            trusted_proxies,
            cdn_proxies,
            cdn_header,
        })
    }

    /// Reports whether the app runs in production.
    pub fn is_production(&self) -> bool {
        self.app_env == Environment::Production
    }
}

/// Reads the first of [`CORS_ORIGIN_KEYS`] that is set. Unset means no cross-origin caller.
pub fn cors_origins(vars: &Vars) -> Vec<String> {
    CORS_ORIGIN_KEYS.iter().find(|key| vars.get(key).is_some()).map(|key| vars.list(key)).unwrap_or_default()
}

pub use proxies::{cdn, trusted as trusted_proxies};
