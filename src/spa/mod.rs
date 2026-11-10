//! Serves a built single-page application from a directory, as Go `spa`.
//!
//! Mount it last, as the router's fallback: `router.fallback_service(spa::handler(config))`.

mod files;
mod hashes;
mod scan;

use std::path::{Path, PathBuf};

use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, any};
use http::{HeaderValue, Method, StatusCode, header};
use tower::ServiceExt;
use tower_http::services::ServeFile;

pub use files::IMMUTABLE_PREFIXES;
pub use hashes::inline_script_hashes;

use crate::http::{not_found, plain};

/// Where the container image places the built client.
pub const DEFAULT_DIR: &str = "./client";

/// What to serve and from where.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// The built client. Empty means [`DEFAULT_DIR`].
    pub dir: PathBuf,
    /// The history-fallback document. Empty means `index.html`.
    pub index: String,
}

/// Reads `CLIENT_DIR`, falling back to [`DEFAULT_DIR`].
pub fn dir_from_env() -> PathBuf {
    let dir = std::env::var("CLIENT_DIR").unwrap_or_default();
    PathBuf::from(if dir.trim().is_empty() { DEFAULT_DIR } else { dir.trim() })
}

/// Reports whether `dir` holds a built client, so an app can skip mounting the handler.
pub fn available(dir: &Path) -> bool {
    dir.join("index.html").is_file()
}

/// Serves files from the directory and falls back to the index document for client routes.
///
/// A path with an extension never falls back, so a missing bundle is a 404 rather than HTML.
/// Dotfiles are hidden. Only GET and HEAD are served; any other verb is a 405 naming
/// `Allow: GET, HEAD`, so a machine client cannot mistake the index for success.
pub fn handler<S: Clone + Send + Sync + 'static>(config: Config) -> MethodRouter<S> {
    let dir = if config.dir.as_os_str().is_empty() { PathBuf::from(DEFAULT_DIR) } else { config.dir };
    let index = if config.index.is_empty() { "index.html".to_owned() } else { config.index };
    any(move |request: Request| {
        let (dir, index) = (dir.clone(), index.clone());
        async move { serve(&dir, &index, request).await }
    })
}

async fn serve(dir: &Path, index: &str, request: Request) -> Response {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        let mut response = plain(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n");
        response.headers_mut().insert(header::ALLOW, HeaderValue::from_static("GET, HEAD"));
        return response;
    }
    let Some(decoded) = files::decode_path(request.uri().path()) else { return not_found().await };
    let clean = files::clean(&decoded);
    if clean.rsplit('/').next().is_some_and(|base| base.starts_with('.')) {
        return not_found().await;
    }
    let target = files::resolve(dir, &clean);
    if target.is_file() {
        if decoded.ends_with("/index.html") {
            let mut response = (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, "./")]).into_response();
            files::set(&mut response, header::CACHE_CONTROL, files::cache_policy(&clean));
            return response;
        }
        return file(&target, request, files::cache_policy(&clean), files::content_type(&clean)).await;
    }
    if files::has_extension(&clean) || !dir.join(index).is_file() {
        return not_found().await;
    }
    file(&dir.join(index), request, "no-cache", Some("text/html; charset=utf-8")).await
}

async fn file(path: &Path, request: Request, cache: &'static str, content_type: Option<&'static str>) -> Response {
    let mut response = match ServeFile::new(path).oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(_) => return not_found().await,
    };
    files::set(&mut response, header::CACHE_CONTROL, cache);
    if let Some(content_type) = content_type.filter(|_| response.headers().contains_key(header::CONTENT_TYPE)) {
        files::set(&mut response, header::CONTENT_TYPE, content_type);
    }
    response
}
