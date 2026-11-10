use std::collections::{BTreeMap, BTreeSet};

use axum::Router;
use utoipa::openapi::OpenApi;

use super::{BASE_PATH, Config};

/// Reports routes mounted on `router` that `doc` does not document, as sorted `"METHOD /path"`
/// strings, as Go `apiref.Undocumented`. A documented path also matches under each of
/// `config.servers`. The reference page, the health probes, wildcard routes and any `ignore`
/// prefix are skipped. A route that answers every method, such as `any(...)`, is `ANY /path`.
pub fn undocumented<S>(router: &Router<S>, doc: &OpenApi, config: &Config, ignore: &[&str]) -> Vec<String> {
    let documented = documented(doc, &config.servers);
    let mut skip = vec![BASE_PATH, "/health", "/ready", "/api/health", "/api/ready"];
    skip.extend_from_slice(ignore);
    let mut missing = BTreeSet::new();
    for (path, methods) in mounted(router) {
        if path.contains('*') || skip.iter().any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/"))) {
            continue;
        }
        for method in methods {
            let route = format!("{method} {}", normalize(&path));
            if !documented.contains(&route) {
                missing.insert(route);
            }
        }
    }
    missing.into_iter().collect()
}

fn documented(doc: &OpenApi, servers: &[String]) -> BTreeSet<String> {
    let mut routes = BTreeSet::new();
    for (path, item) in &doc.paths.paths {
        let operations = [
            ("GET", &item.get),
            ("POST", &item.post),
            ("PUT", &item.put),
            ("PATCH", &item.patch),
            ("DELETE", &item.delete),
        ];
        for (method, _) in operations.iter().filter(|(_, operation)| operation.is_some()) {
            routes.insert(format!("{method} {}", normalize(path)));
            for server in servers {
                routes.insert(format!("{method} {}", normalize(&format!("{}{path}", server.trim_end_matches('/')))));
            }
        }
    }
    routes
}

fn normalize(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() { "/".to_owned() } else { trimmed.to_owned() }
}

/// The routes axum registered, path to methods, read from the router's `Debug` output: axum
/// offers no other way to list them, and this reads them without running a handler. `HEAD` is
/// left out, since every `GET` route answers it. `tests/apiref.rs` fails if a new axum release
/// changes that output.
pub fn mounted<S>(router: &Router<S>) -> BTreeMap<String, Vec<String>> {
    let debug = format!("{router:?}");
    let section = debug.split("fallback_router:").next().unwrap_or("");
    let (routes, paths) = section.split_once("node: Node").unwrap_or((section, ""));
    let allowed: BTreeMap<&str, Vec<String>> = entries(routes).map(|(id, rest)| (id, methods(rest))).collect();
    entries(paths)
        .filter_map(|(id, rest)| {
            let path = rest.strip_prefix(": \"")?.split('"').next()?;
            Some((path.to_owned(), allowed.get(id).cloned().unwrap_or_default()))
        })
        .collect()
}

fn entries(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split("RouteId(").skip(1).filter_map(|chunk| chunk.split_once(')'))
}

fn methods(endpoint: &str) -> Vec<String> {
    let endpoint = endpoint.split("RouteId(").next().unwrap_or(endpoint);
    let Some(rest) = endpoint.split_once("allow_header: Bytes(b\"").map(|(_, rest)| rest) else {
        return vec!["ANY".to_owned()];
    };
    let header = rest.split('"').next().unwrap_or("");
    header.split(',').filter(|m| !m.is_empty() && *m != "HEAD").map(str::to_owned).collect()
}
