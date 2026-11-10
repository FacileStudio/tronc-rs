use std::collections::BTreeMap;

pub use super::directives::{extend, parse, render};

/// Directive name to sources, as `SecurityConfig::csp_sources` takes them.
pub type Sources = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub name: String,
    pub sources: Vec<String>,
}

fn directive(name: &str, sources: &[&str]) -> Directive {
    Directive { name: name.to_owned(), sources: sources.iter().map(|s| (*s).to_owned()).collect() }
}

fn default_policy() -> Vec<Directive> {
    vec![
        directive("default-src", &["'self'"]),
        directive("base-uri", &["'self'"]),
        directive("object-src", &["'none'"]),
        directive("frame-ancestors", &["'none'"]),
        directive("form-action", &["'self'"]),
        directive("img-src", &["'self'", "data:", "blob:", "https:"]),
        directive("connect-src", &["'self'"]),
        directive("script-src", &["'self'"]),
        directive("style-src", &["'self'", "'unsafe-inline'"]),
    ]
}

/// Renders the default policy extended by `extra`, plus `report-uri` when set, as Go
/// `middleware.BuildCSP`. Sources extend a directive and replace a `'none'` default; a directive
/// the default lacks is appended in name order.
pub fn build_csp(extra: &Sources, report_uri: &str) -> Result<String, String> {
    Ok(render(&build(extra, report_uri)?))
}

pub fn build(extra: &Sources, report_uri: &str) -> Result<Vec<Directive>, String> {
    validate(extra, report_uri)?;
    let mut directives = extend(default_policy(), extra, true);
    if !report_uri.is_empty() {
        directives.push(directive("report-uri", &[report_uri]));
    }
    Ok(directives)
}

pub fn framing(directives: &[Directive]) -> Vec<Directive> {
    ["frame-ancestors", "object-src", "base-uri"]
        .iter()
        .filter_map(|name| directives.iter().find(|d| d.name == *name).cloned())
        .collect()
}

pub fn validate(extra: &Sources, report_uri: &str) -> Result<(), String> {
    for (name, sources) in extra {
        if !token_name(name) {
            return Err(format!("tronc/middleware: CSP directive {name:?} is not a directive name"));
        }
        if let Some(bad) = sources.iter().find(|source| !valid_source(source)) {
            return Err(format!("tronc/middleware: CSP source {bad:?} would corrupt the policy"));
        }
    }
    if !report_uri.is_empty() && !valid_source(report_uri) {
        return Err(format!("tronc/middleware: CSP report-uri {report_uri:?} would corrupt the policy"));
    }
    Ok(())
}

pub fn token_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_lowercase() || c == '-')
}

fn valid_source(source: &str) -> bool {
    !source.is_empty() && !source.chars().any(|c| ";, \t\r\n".contains(c) || c.is_control())
}
