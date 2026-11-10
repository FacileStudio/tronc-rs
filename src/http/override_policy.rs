use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use http::{HeaderMap, HeaderName, HeaderValue, header};

use super::csp::{self, Sources};
use super::policy;
use super::security::framing_opened;

const CSP: HeaderName = header::CONTENT_SECURITY_POLICY;
const CSP_REPORT_ONLY: HeaderName = header::CONTENT_SECURITY_POLICY_REPORT_ONLY;

/// Changes the chassis headers for one route, as Go `middleware.SecurityOverrideConfig`.
/// Every field extends what the chassis sent; none replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecurityOverrideConfig {
    /// `Some(true)` promotes a report-only policy to an enforced one. A route never downgrades.
    pub csp_enforce: Option<bool>,
    /// Sources extending the chassis policy. Opening `frame-ancestors` drops X-Frame-Options.
    pub csp_sources: Sources,
    /// Permissions-Policy features granted to `(self)` on top of the chassis grants.
    pub allow: Vec<String>,
    /// Replaces the Cross-Origin-Opener-Policy when set.
    pub cross_origin_opener_policy: String,
}

/// A compiled route override. Mount it on the route with
/// `axum::middleware::from_fn_with_state(Arc::new(o), security_override)`; the chassis applies
/// it after its own headers, so the route keeps the app's mode, hashes and report-uri.
#[derive(Debug, Clone)]
pub struct SecurityOverride {
    config: SecurityOverrideConfig,
}

impl SecurityOverride {
    /// Validates `config` with the same rules the chassis applies.
    pub fn new(config: SecurityOverrideConfig) -> Result<Self, String> {
        csp::validate(&config.csp_sources, "")?;
        policy::permissions(&config.allow)?;
        if !config.cross_origin_opener_policy.is_empty() {
            policy::opener(&config.cross_origin_opener_policy)?;
        }
        Ok(Self { config })
    }

    /// Extends the headers the chassis set on a response.
    pub fn apply(&self, headers: &mut HeaderMap) {
        self.apply_csp(headers);
        if framing_opened(self.config.csp_sources.get("frame-ancestors")) {
            headers.remove(header::X_FRAME_OPTIONS);
        }
        if !self.config.allow.is_empty() {
            let current = text(headers.get(header::HeaderName::from_static("permissions-policy")));
            set(headers, HeaderName::from_static("permissions-policy"), &policy::grant(&current, &self.config.allow));
        }
        if !self.config.cross_origin_opener_policy.is_empty() {
            set(
                headers,
                HeaderName::from_static("cross-origin-opener-policy"),
                &self.config.cross_origin_opener_policy,
            );
        }
    }

    fn apply_csp(&self, headers: &mut HeaderMap) {
        let extended =
            |policy: &str, add: bool| csp::render(&csp::extend(csp::parse(policy), &self.config.csp_sources, add));
        let Some(report_only) = headers.get(CSP_REPORT_ONLY).map(|v| text(Some(v))) else {
            if let Some(enforced) = headers.get(CSP).map(|v| text(Some(v))) {
                set(headers, CSP, &extended(&enforced, true));
            }
            return;
        };
        let full = extended(&report_only, true);
        if self.config.csp_enforce == Some(true) {
            headers.remove(CSP_REPORT_ONLY);
            set(headers, CSP, &full);
            return;
        }
        set(headers, CSP_REPORT_ONLY, &full);
        if let Some(enforced) = headers.get(CSP).map(|v| text(Some(v))) {
            set(headers, CSP, &extended(&enforced, false));
        }
    }
}

fn text(value: Option<&HeaderValue>) -> String {
    value.and_then(|v| v.to_str().ok()).unwrap_or("").to_owned()
}

fn set(headers: &mut HeaderMap, name: HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

#[derive(Clone)]
pub struct Pending(pub Arc<SecurityOverride>);

/// The route middleware for a [`SecurityOverride`].
pub async fn security_override(State(o): State<Arc<SecurityOverride>>, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response.extensions_mut().insert(Pending(o));
    response
}
