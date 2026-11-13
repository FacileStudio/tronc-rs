use std::sync::Arc;

use axum::extract::{Path, Request, State};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use serde::Serialize;

use super::confirmations::STATUS_DONE;
use super::pending::OUTCOME_DISMISSED;
use super::pending_page::{PendingPage, PendingResult};
use crate::errors::Result;

type Page<T> = State<Arc<PendingPage<T>>>;

fn answer<V: Serialize>(result: Result<V>) -> Response {
    match result {
        Ok(body) => crate::json::write(StatusCode::OK, &body),
        Err(error) => error.into_response(),
    }
}

pub(super) async fn list<T: Serialize + Send + 'static>(State(page): Page<T>, request: Request) -> Response {
    let listed = async {
        let (_, who) = page.caller(request).await?;
        page.list(&who.user_id).await
    };
    answer(listed.await)
}

pub(super) async fn confirm<T: Serialize + Send + 'static>(
    State(page): Page<T>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let confirmed = async {
        let (confirmations, who) = page.caller(request).await?;
        let outcome = confirmations.confirm(&who, &id).await?;
        Ok(match &page.done {
            Some(done) => done(outcome).await,
            None => PendingResult { status: STATUS_DONE.to_owned(), url: String::new() },
        })
    };
    answer(confirmed.await)
}

pub(super) async fn dismiss<T: Serialize + Send + 'static>(
    State(page): Page<T>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    let dismissed = async {
        let (confirmations, who) = page.caller(request).await?;
        confirmations.dismiss(&who, &id).await?;
        Ok(PendingResult { status: OUTCOME_DISMISSED.to_owned(), url: String::new() })
    };
    answer(dismissed.await)
}
