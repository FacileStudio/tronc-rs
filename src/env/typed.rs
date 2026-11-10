use std::time::Duration;

use super::{EnvError, Vars, duration};

impl Vars {
    /// Reads `key` as an integer.
    pub fn int(&self, key: &str, fallback: i64) -> Result<i64, EnvError> {
        self.parsed(key, fallback, "an integer", |raw| raw.parse().ok())
    }

    /// Reads `key` as a boolean, accepting Go's `strconv.ParseBool` spellings.
    pub fn bool(&self, key: &str, fallback: bool) -> Result<bool, EnvError> {
        self.parsed(key, fallback, "a boolean", |raw| match raw {
            "1" | "t" | "T" | "TRUE" | "true" | "True" => Some(true),
            "0" | "f" | "F" | "FALSE" | "false" | "False" => Some(false),
            _ => None,
        })
    }

    /// Reads `key` as a Go duration, such as `30s`, `5m` or `1h30m`.
    pub fn duration(&self, key: &str, fallback: Duration) -> Result<Duration, EnvError> {
        self.parsed(key, fallback, "a duration", duration::parse)
    }

    fn parsed<T>(&self, key: &str, fallback: T, kind: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T, EnvError> {
        let Some(raw) = self.get(key) else { return Ok(fallback) };
        parse(&raw).ok_or_else(|| EnvError(format!("{key} must be {kind}, got {raw:?}")))
    }
}
