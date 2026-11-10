//! The live API reference, as Go `apiref`: a Scalar page at `/docs` and the OpenAPI 3.1 document
//! at `/docs/openapi.json`, built from a `utoipa` document.
//!
//! Build the API with `utoipa_axum::router::OpenApiRouter` and `routes!`, split it with
//! `split_for_parts`, and mount [`routes`] on the root router before the SPA fallback. Then keep
//! the drift test: `assert!(apiref::undocumented(&app, &doc, &config, &[]).is_empty())`.

mod coverage;
mod page;

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse;
use axum::routing::get;
use http::{StatusCode, header};
use utoipa::openapi::{OpenApi, Server};

pub use coverage::{mounted, undocumented};

use crate::http::{SecurityOverride, SecurityOverrideConfig, security_override};
use crate::json;

/// The pinned Scalar bundle, the same build Go `tronc` pins.
pub const SCALAR_SCRIPT_URL: &str = "https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.64.0";
/// Where the Scalar bundle loads its fonts from.
pub const SCALAR_FONT_ORIGIN: &str = "https://fonts.scalar.com";
/// Where the reference page is mounted.
pub const BASE_PATH: &str = "/docs";
/// Where the OpenAPI document is mounted.
pub const SPEC_PATH: &str = "/docs/openapi.json";
/// The API version reported when [`Config::version`] is empty.
pub const DEFAULT_VERSION: &str = "1.0.0";

/// One API's reference page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Names the API, as in `Feuilles API`. Empty means `API`.
    pub title: String,
    /// The API version. Empty means [`DEFAULT_VERSION`].
    pub version: String,
    /// One or two sentences shown above the route list.
    pub description: String,
    /// The base URLs every documented path is relative to, usually `["/api"]`.
    pub servers: Vec<String>,
    /// Overrides the Scalar bundle, for an air-gapped deployment. Empty means the pinned one.
    pub script_url: String,
}

/// Stamps the configured title, version, description and servers onto `doc`.
pub fn document(config: &Config, mut doc: OpenApi) -> OpenApi {
    doc.info.title = title(config);
    doc.info.version = if config.version.is_empty() { DEFAULT_VERSION.to_owned() } else { config.version.clone() };
    if !config.description.is_empty() {
        doc.info.description = Some(config.description.clone());
    }
    if !config.servers.is_empty() {
        doc.servers = Some(config.servers.iter().map(Server::new).collect());
    }
    doc
}

/// The `/docs` page and `/docs/openapi.json`. The page widens the app's CSP to the Scalar
/// bundle's exact URL and its fonts, on that route only.
pub fn routes<S: Clone + Send + Sync + 'static>(config: &Config, doc: OpenApi) -> Result<Router<S>, String> {
    let script_url = if config.script_url.is_empty() { SCALAR_SCRIPT_URL } else { config.script_url.as_str() };
    let spec = json::encode(&document(config, doc));
    let html = page::render(&title(config), SPEC_PATH, script_url);
    let override_config = SecurityOverrideConfig { csp_sources: page::sources(script_url), ..Default::default() };
    let widen = from_fn_with_state(Arc::new(SecurityOverride::new(override_config)?), security_override);
    let page = get(move || page::respond(html.clone())).layer(widen);
    let spec = get(move || {
        let headers = [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "public, max-age=3600")];
        std::future::ready((StatusCode::OK, headers, spec.clone()).into_response())
    });
    Ok(Router::new().route(BASE_PATH, page).route(SPEC_PATH, spec))
}

fn title(config: &Config) -> String {
    if config.title.is_empty() { "API".to_owned() } else { config.title.clone() }
}
