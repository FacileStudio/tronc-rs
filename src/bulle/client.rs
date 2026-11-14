use std::time::{Duration, SystemTime};

use reqwest::{Url, header};
use serde::de::DeserializeOwned;

use super::error::Error;
use super::request::{Request, TokenSource};
use super::wire::{answer, decode, retry_after, unreachable};

/// How long the default HTTP client waits for Bulle, as Go `tronc/bulle`.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// The largest answer read from Bulle, 1 MiB. A longer one is cut and fails to decode.
pub const MAX_ANSWER_BYTES: usize = 1 << 20;

const MAX_SEGMENT_BYTES: usize = 200;

/// Calls Bulle's API as a user. Cheap to clone and safe to share: build one at boot and clone
/// it, so every call reuses its connection pool.
#[derive(Debug, Clone)]
pub struct Client {
    base: Url,
    http: reqwest::Client,
}

impl Client {
    /// A client for Bulle at `url`, such as `https://bulle.facile.studio`, with a
    /// [`DEFAULT_TIMEOUT`] HTTP client that never follows a redirect.
    pub fn new(url: &str) -> Result<Self, String> {
        Self::with_timeout(url, DEFAULT_TIMEOUT)
    }

    /// [`Self::new`] with another timeout, such as a longer one for completions.
    pub fn with_timeout(url: &str, timeout: Duration) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| format!("bulle: building the HTTP client: {e}"))?;
        Self::with_http(url, http)
    }

    /// A client for Bulle at `url` that sends through `http`, which owns the timeouts and the
    /// redirect policy. A redirect is answered as an error unless `http` follows it, so a token
    /// never reaches another host.
    pub fn with_http(url: &str, http: reqwest::Client) -> Result<Self, String> {
        let invalid = || "bulle: URL must be an absolute http or https origin".to_owned();
        let base = Url::parse(url.trim_end_matches('/')).map_err(|_| invalid())?;
        let usable = matches!(base.scheme(), "http" | "https")
            && base.host_str().is_some_and(|h| !h.is_empty())
            && base.username().is_empty()
            && base.password().is_none()
            && base.query().is_none()
            && base.fragment().is_none();
        if !usable {
            return Err(invalid());
        }
        Ok(Self { base, http })
    }

    /// Bulle's origin, with no trailing slash.
    pub fn base_url(&self) -> String {
        self.base.as_str().trim_end_matches('/').to_owned()
    }

    /// Where the user continues a run's conversation in Bulle, or Bulle's origin without a
    /// session.
    pub fn continue_url(&self, session_id: &str) -> String {
        if session_id.is_empty() {
            return self.base_url();
        }
        let mut url = self.base.clone();
        if let Ok(mut path) = url.path_segments_mut() {
            path.pop_if_empty().push("s").push(session_id);
        }
        url.to_string()
    }

    /// Sends `request` with the token from `tokens` and decodes a 2xx JSON answer into `T`. An
    /// empty answer decodes as JSON `null`, so `()`, `Option<_>` and `serde::de::IgnoredAny`
    /// accept it. A 401 retries once when [`TokenSource::forget`] asks for it.
    pub async fn call<T, S>(&self, tokens: &S, request: Request) -> Result<T, Error>
    where
        T: DeserializeOwned,
        S: TokenSource + ?Sized,
    {
        let target = self.target(&request)?;
        let first = self.send(tokens, &request, target.clone()).await;
        let retry = match &first {
            Err(Error::Status { status: 401, .. }) => tokens.forget(),
            _ => None,
        };
        let raw = match retry {
            Some(forget) => {
                forget.await;
                self.send(tokens, &request, target).await?
            }
            None => first?,
        };
        decode(&raw)
    }

    fn target(&self, request: &Request) -> Result<Url, Error> {
        let bad = |s: &String| s.is_empty() || s == "." || s == ".." || s.len() > MAX_SEGMENT_BYTES;
        if request.segments.iter().any(bad) {
            return Err(Error::BadSegment);
        }
        let mut url = self.base.clone();
        url.path_segments_mut().map_err(|_| Error::BadSegment)?.pop_if_empty().push("api").extend(&request.segments);
        if !request.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&request.query);
        }
        Ok(url)
    }

    async fn send<S: TokenSource + ?Sized>(
        &self,
        tokens: &S,
        request: &Request,
        target: Url,
    ) -> Result<Vec<u8>, Error> {
        let token = tokens.token().await.map_err(Error::Token)?;
        let mut builder = self
            .http
            .request(request.method.clone(), target)
            .bearer_auth(token)
            .header(header::ACCEPT, "application/json");
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if let Some(body) = &request.body {
            builder = builder.header(header::CONTENT_TYPE, "application/json").body(body.to_string());
        }
        let mut response = builder.send().await.map_err(unreachable)?;
        let status = response.status();
        let wait = retry_after(response.headers(), SystemTime::now());
        let mut raw = Vec::new();
        while raw.len() < MAX_ANSWER_BYTES {
            match response.chunk().await.map_err(unreachable)? {
                Some(chunk) => raw.extend_from_slice(&chunk),
                None => break,
            }
        }
        raw.truncate(MAX_ANSWER_BYTES);
        answer(status, wait, raw)
    }
}
