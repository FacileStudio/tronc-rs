use axum::response::{IntoResponse, Response};
use http::header;

use super::SCALAR_FONT_ORIGIN;
use crate::http::{Sources, build_csp};

/// The CSP sources the page needs: the bundle's exact URL, never its whole host, and Scalar's
/// fonts. A script URL that is not plain `https` or would corrupt the policy is left out.
pub fn sources(script_url: &str) -> Sources {
    let mut sources = Sources::new();
    sources.insert("font-src".into(), vec!["'self'".into(), SCALAR_FONT_ORIGIN.into()]);
    let Ok(uri) = script_url.parse::<http::Uri>() else { return sources };
    let authority = uri.authority().map(|a| a.as_str()).unwrap_or("");
    if uri.scheme_str() != Some("https") || authority.is_empty() || authority.contains('@') {
        return sources;
    }
    let mut with_script = sources.clone();
    with_script.insert("script-src".into(), vec![format!("https://{authority}{}", uri.path())]);
    if build_csp(&with_script, "").is_err() {
        return sources;
    }
    with_script
}

fn escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// The Scalar page, as Go's template renders it.
pub fn render(title: &str, spec_url: &str, script_url: &str) -> String {
    format!(
        "<!doctype html>\n<html>\n<head>\n  <title>{}</title>\n  <meta charset=\"utf-8\" />\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n  <style>body {{ margin: 0; }}</style>\n</head>\n<body>\n  <script id=\"api-reference\" data-url=\"{}\"></script>\n  <script src=\"{}\"></script>\n</body>\n</html>",
        escape(title),
        escape(spec_url),
        escape(script_url)
    )
}

pub async fn respond(html: String) -> Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], html).into_response()
}
