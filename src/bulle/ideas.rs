use http::Method;
use reqwest::Url;
use serde::de::IgnoredAny;
use serde_json::json;

use super::client::Client;
use super::error::Error;
use super::request::{Request, TokenSource};
use super::types::{Batch, IdeaList, Run, RunStart, run_state};

impl Client {
    /// Lists `app`'s open ideas in `space`: a Bulle space id, the id of the Registre shared
    /// space it mirrors, or `""` for the user's personal space.
    pub async fn ideas<S: TokenSource + ?Sized>(&self, tokens: &S, app: &str, space: &str) -> Result<IdeaList, Error> {
        let request = Request::new(Method::GET, ["ideas"]).query("app", app).query("space_id", space);
        self.call(tokens, request).await
    }

    /// Starts a batch of `app`'s ideas in `space`. Bulle caps it at three a day per user and
    /// app, and answers 429 past that and 409 while one runs.
    pub async fn generate<S: TokenSource + ?Sized>(&self, tokens: &S, app: &str, space: &str) -> Result<Batch, Error> {
        let request =
            Request::new(Method::POST, ["ideas", "generate"]).query("app", app).json(json!({ "space_id": space }));
        self.call(tokens, request).await
    }

    /// Starts the idea as a background run. With a space, Bulle refuses an idea of another space.
    pub async fn run<S: TokenSource + ?Sized>(
        &self,
        tokens: &S,
        idea_id: &str,
        space: &str,
    ) -> Result<RunStart, Error> {
        let request = Request::new(Method::POST, ["ideas", idea_id, "run"]).query("space_id", space);
        self.call(tokens, request).await
    }

    /// Reads a run: its state, summary, outputs and pending approvals, with `state` and
    /// `continue_url` filled in and every output URL that is not `http` or `https` emptied.
    pub async fn run_status<S: TokenSource + ?Sized>(&self, tokens: &S, run_id: &str) -> Result<Run, Error> {
        let mut run: Run = self.call(tokens, Request::new(Method::GET, ["runs", run_id])).await?;
        run.state = run_state(&run.status, run.pending_approvals.len()).to_owned();
        run.continue_url = self.continue_url(&run.session_id);
        for output in &mut run.outputs {
            output.url = web_link(&output.url);
        }
        Ok(run)
    }

    /// Approves or declines a call a run is waiting on.
    pub async fn decide<S: TokenSource + ?Sized>(
        &self,
        tokens: &S,
        run_id: &str,
        call_id: &str,
        approve: bool,
    ) -> Result<(), Error> {
        let request =
            Request::new(Method::POST, ["runs", run_id, "approvals", call_id]).json(json!({ "approve": approve }));
        self.call::<IgnoredAny, S>(tokens, request).await.map(drop)
    }

    /// Hides an idea for good.
    pub async fn dismiss<S: TokenSource + ?Sized>(&self, tokens: &S, idea_id: &str) -> Result<(), Error> {
        let request = Request::new(Method::PATCH, ["ideas", idea_id]).json(json!({ "status": "dismissed" }));
        self.call::<IgnoredAny, S>(tokens, request).await.map(drop)
    }
}

fn web_link(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(url)
            if matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some_and(|h| !h.is_empty())
                && url.username().is_empty()
                && url.password().is_none() =>
        {
            url.to_string()
        }
        _ => String::new(),
    }
}
