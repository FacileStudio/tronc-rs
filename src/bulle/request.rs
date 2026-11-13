use std::future::{Future, ready};
use std::pin::Pin;

use http::Method;
use serde_json::Value;

/// The future a [`TokenSource`] returns its bearer token in.
pub type TokenFuture<'a> = Pin<Box<dyn Future<Output = Result<String, crate::Error>> + Send + 'a>>;

/// The future [`TokenSource::forget`] runs in.
pub type ForgetFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Gives the bearer token of the user a call is made for. In the suite it is a token exchanged
/// by porte for audience `bulle` with scopes `bulle.read` and `bulle.write`; tronc never
/// depends on porte. A plain `str` or `String` is a fixed token.
pub trait TokenSource: Send + Sync {
    /// The bearer token.
    fn token(&self) -> TokenFuture<'_>;

    /// Drops a cached token after Bulle answered 401. When it returns a future, the client runs
    /// it and retries the call once; the default returns `None`, so a fixed token never retries.
    fn forget(&self) -> Option<ForgetFuture<'_>> {
        None
    }
}

impl TokenSource for str {
    fn token(&self) -> TokenFuture<'_> {
        Box::pin(ready(Ok(self.to_owned())))
    }
}

impl TokenSource for String {
    fn token(&self) -> TokenFuture<'_> {
        self.as_str().token()
    }
}

/// One call to Bulle's API. Segments follow `/api/` and are escaped one by one, so an id holding
/// a slash or a query stays one segment.
#[derive(Debug, Clone)]
pub struct Request {
    /// The HTTP method.
    pub method: Method,
    /// The path after `/api/`, one segment per item.
    pub segments: Vec<String>,
    /// Query parameters, form-encoded in order.
    pub query: Vec<(String, String)>,
    /// The JSON body, if any.
    pub body: Option<Value>,
    /// Extra request headers, such as [`super::ACTING_FOR_HEADER`].
    pub headers: Vec<(String, String)>,
}

impl Request {
    /// A call to `/api/<segments...>` with no query and no body.
    pub fn new<I: IntoIterator<Item = S>, S: Into<String>>(method: Method, segments: I) -> Self {
        Self {
            method,
            segments: segments.into_iter().map(Into::into).collect(),
            query: Vec::new(),
            body: None,
            headers: Vec::new(),
        }
    }

    /// Adds the query parameter `key=value` unless `value` is empty.
    pub fn query(mut self, key: &str, value: &str) -> Self {
        if !value.is_empty() {
            self.query.push((key.to_owned(), value.to_owned()));
        }
        self
    }

    /// Adds the request header `name: value`.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Sets the JSON body.
    pub fn json(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }
}
