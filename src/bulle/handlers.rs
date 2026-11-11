use axum::response::{IntoResponse, Response};
use http::StatusCode;
use http::request::Parts;
use serde::Serialize;

use super::error::Error;
use super::router::{Ideas, IdeasPage};
use super::types::IdeaList;
use crate::json::write;

fn respond<T: Serialize>(status: StatusCode, result: Result<T, Error>) -> Response {
    match result {
        Ok(body) => write(status, &body),
        Err(error) => crate::Error::from(error).into_response(),
    }
}

impl Ideas {
    pub(super) async fn list(&self, parts: Parts) -> Response {
        if !self.ai_access(&parts) {
            let upgrade_url = self.options.upgrade_url.clone();
            return write(StatusCode::OK, &IdeasPage { ai_access: false, upgrade_url, list: IdeaList::default() });
        }
        let tokens = (self.options.tokens)(&parts);
        let list = self.options.client.ideas(&*tokens, &self.options.app, &self.space(&parts)).await;
        respond(StatusCode::OK, list.map(|list| IdeasPage { ai_access: true, upgrade_url: String::new(), list }))
    }

    pub(super) async fn generate(&self, parts: Parts) -> Response {
        if let Err(error) = self.require_ai(&parts) {
            return error.into_response();
        }
        let tokens = (self.options.tokens)(&parts);
        respond(
            StatusCode::ACCEPTED,
            self.options.client.generate(&*tokens, &self.options.app, &self.space(&parts)).await,
        )
    }

    pub(super) async fn run(&self, parts: Parts, id: String) -> Response {
        if let Err(error) = self.require_ai(&parts) {
            return error.into_response();
        }
        let tokens = (self.options.tokens)(&parts);
        respond(StatusCode::ACCEPTED, self.options.client.run(&*tokens, &id, &self.space(&parts)).await)
    }

    pub(super) async fn dismiss(&self, parts: Parts, id: String) -> Response {
        let tokens = (self.options.tokens)(&parts);
        match self.options.client.dismiss(&*tokens, &id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => crate::Error::from(error).into_response(),
        }
    }

    pub(super) async fn status(&self, parts: Parts, id: String) -> Response {
        let tokens = (self.options.tokens)(&parts);
        respond(StatusCode::OK, self.options.client.run_status(&*tokens, &id).await)
    }

    pub(super) async fn decide(&self, parts: Parts, (run, call): (String, String), approve: bool) -> Response {
        if approve && let Err(error) = self.require_ai(&parts) {
            return error.into_response();
        }
        let tokens = (self.options.tokens)(&parts);
        match self.options.client.decide(&*tokens, &run, &call, approve).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => crate::Error::from(error).into_response(),
        }
    }
}
