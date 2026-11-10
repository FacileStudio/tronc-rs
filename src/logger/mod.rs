//! The suite's structured logger: JSON lines on stdout through `tracing`, one level parsed from
//! configuration, and, with the `journal` feature, a second layer shipping records to Journal.

use tracing::Level;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::env::Core;

/// The logger an app wants. The default is info-level JSON on stdout.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// One of `debug`, `info`, `warn`, `error`. Anything else means info.
    pub level: String,
    /// Where to ship records, when set.
    #[cfg(feature = "journal")]
    pub journal: Option<crate::journal::Config>,
}

impl Config {
    /// The logger the core configuration asks for: its level, and Journal shipping when both
    /// `JOURNAL_URL` and `JOURNAL_TOKEN` are set.
    pub fn from_core(core: &Core) -> Self {
        Self {
            level: core.log_level.clone(),
            #[cfg(feature = "journal")]
            journal: (!core.journal_url.is_empty() && !core.journal_token.is_empty())
                .then(|| crate::journal::Config::new(&core.journal_url, &core.journal_token)),
        }
    }
}

/// Keeps the log pipeline alive. Dropping it flushes what is buffered for Journal.
#[must_use = "dropping the guard stops log shipping"]
#[derive(Debug, Default)]
pub struct Guard {
    #[cfg(feature = "journal")]
    journal: Option<crate::journal::Client>,
}

#[cfg(feature = "journal")]
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(client) = self.journal.take() {
            client.close();
        }
    }
}

/// Maps a configured level name onto a level, defaulting to info.
pub fn parse_level(level: &str) -> Level {
    match level.trim().to_ascii_lowercase().as_str() {
        "debug" => Level::DEBUG,
        "warn" | "warning" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    }
}

/// Installs the global subscriber. A second call keeps the first subscriber and returns a guard
/// that ships nothing.
pub fn init(config: Config) -> Guard {
    let filter = LevelFilter::from_level(parse_level(&config.level));
    let stdout =
        tracing_subscriber::fmt::layer().json().flatten_event(true).with_current_span(false).with_target(false);
    #[cfg(feature = "journal")]
    {
        let client = config.journal.map(crate::journal::Client::new);
        let layer = client.as_ref().map(crate::journal::Client::layer);
        if tracing_subscriber::registry().with(filter).with(stdout).with(layer).try_init().is_err() {
            client.iter().for_each(crate::journal::Client::close);
            return Guard { journal: None };
        }
        Guard { journal: client }
    }
    #[cfg(not(feature = "journal"))]
    {
        let _ = tracing_subscriber::registry().with(filter).with(stdout).try_init();
        Guard {}
    }
}
