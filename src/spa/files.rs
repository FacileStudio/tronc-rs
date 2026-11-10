use std::path::{Path, PathBuf};

use axum::response::Response;
use http::{HeaderName, HeaderValue};
use percent_encoding::percent_decode_str;

/// URL prefixes whose files carry a content hash and are cached forever.
pub const IMMUTABLE_PREFIXES: [&str; 3] = ["/_app/immutable/", "/assets/", "/_nuxt/"];

const CONTENT_TYPES: [(&str, &str); 22] = [
    ("avif", "image/avif"),
    ("css", "text/css; charset=utf-8"),
    ("gif", "image/gif"),
    ("htm", "text/html; charset=utf-8"),
    ("html", "text/html; charset=utf-8"),
    ("ico", "image/vnd.microsoft.icon"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("js", "text/javascript; charset=utf-8"),
    ("json", "application/json"),
    ("map", "application/json"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("svg", "image/svg+xml"),
    ("txt", "text/plain; charset=utf-8"),
    ("wasm", "application/wasm"),
    ("webmanifest", "application/manifest+json"),
    ("webp", "image/webp"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("xml", "text/xml; charset=utf-8"),
];

pub fn decode_path(raw: &str) -> Option<String> {
    let decoded = percent_decode_str(raw).decode_utf8().ok()?;
    (!decoded.contains('\0')).then(|| decoded.into_owned())
}

pub fn clean(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

pub fn resolve(dir: &Path, clean: &str) -> PathBuf {
    clean.split('/').filter(|s| !s.is_empty()).fold(dir.to_path_buf(), |path, segment| path.join(segment))
}

fn extension(clean: &str) -> Option<&str> {
    let base = clean.rsplit('/').next().unwrap_or("");
    base.rfind('.').map(|dot| &base[dot + 1..])
}

pub fn has_extension(clean: &str) -> bool {
    extension(clean).is_some()
}

pub fn content_type(clean: &str) -> Option<&'static str> {
    let extension = extension(clean)?.to_ascii_lowercase();
    CONTENT_TYPES.iter().find(|(known, _)| *known == extension).map(|(_, value)| *value)
}

pub fn cache_policy(clean: &str) -> &'static str {
    if IMMUTABLE_PREFIXES.iter().any(|prefix| clean.starts_with(prefix)) {
        return "public, max-age=31536000, immutable";
    }
    "public, max-age=0, must-revalidate"
}

pub fn set(response: &mut Response, name: HeaderName, value: &'static str) {
    response.headers_mut().insert(name, HeaderValue::from_static(value));
}
