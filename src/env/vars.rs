use std::collections::HashMap;

use super::EnvError;

type Lookup = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A source of configuration variables: the process environment, or a fixed map in tests.
pub struct Vars {
    lookup: Lookup,
}

impl Vars {
    /// Reads the process environment.
    pub fn process() -> Self {
        Self { lookup: Box::new(|key| std::env::var(key).ok()) }
    }

    /// Reads a fixed set of pairs, so a test never touches the process environment.
    pub fn from_pairs<K: Into<String>, V: Into<String>>(pairs: impl IntoIterator<Item = (K, V)>) -> Self {
        let map: HashMap<String, String> = pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        Self { lookup: Box::new(move |key| map.get(key).cloned()) }
    }

    /// The trimmed value of `key`, or `None` when it is unset or blank.
    pub fn get(&self, key: &str) -> Option<String> {
        (self.lookup)(key).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
    }

    /// Reads `key`, or returns `fallback` when it is unset or blank.
    pub fn string(&self, key: &str, fallback: &str) -> String {
        self.get(key).unwrap_or_else(|| fallback.to_owned())
    }

    /// Reads `key` and fails when it is unset or blank.
    pub fn required(&self, key: &str) -> Result<String, EnvError> {
        self.get(key).ok_or_else(|| EnvError(format!("{key} is required")))
    }

    /// Reads `key` as a comma-separated list, dropping blank entries.
    pub fn list(&self, key: &str) -> Vec<String> {
        let raw = self.get(key).unwrap_or_default();
        raw.split(',').map(str::trim).filter(|part| !part.is_empty()).map(str::to_owned).collect()
    }
}
