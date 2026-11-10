//! Liveness and readiness, as Go `health`.
//!
//! `/health` answers `{"status":"ok"}` as soon as the process serves and touches no dependency.
//! `/ready` runs every check within [`DEFAULT_TIMEOUT`] and answers `{"status":"ready"}` or
//! 503 `{"status":"not_ready"}`. Both are mounted again under `/api`, GET only.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{MethodRouter, get};
use serde_json::json;

use crate::json;

/// How long a readiness probe may take.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

type CheckFuture = Pin<Box<dyn Future<Output = bool> + Send>>;

/// One dependency's readiness test.
#[derive(Clone)]
pub struct Check(Arc<dyn Fn() -> CheckFuture + Send + Sync>);

impl Check {
    /// A check from any async test that reports whether the dependency is usable.
    pub fn new<F, Fut>(check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        Self(Arc::new(move || Box::pin(check())))
    }

    /// A check that pings a Postgres pool.
    pub fn db(pool: sqlx::PgPool) -> Self {
        Self::new(move || {
            let pool = pool.clone();
            async move { sqlx::query("SELECT 1").execute(&pool).await.is_ok() }
        })
    }
}

/// The `/health` and `/ready` routes, at the root and under `/api`.
pub fn routes<S: Clone + Send + Sync + 'static>(checks: Vec<Check>) -> Router<S> {
    let checks = Arc::new(checks);
    let live: MethodRouter<S> = get_only(get(live));
    let ready: MethodRouter<S> = get_only(get(move || readiness(Arc::clone(&checks))));
    Router::new()
        .route("/health", live.clone())
        .route("/ready", ready.clone())
        .route("/api/health", live)
        .route("/api/ready", ready)
}

fn get_only<S: Clone + Send + Sync + 'static>(router: MethodRouter<S>) -> MethodRouter<S> {
    router.head(|| async { StatusCode::METHOD_NOT_ALLOWED })
}

async fn live() -> Response {
    status(StatusCode::OK, "ok")
}

async fn all_ready(checks: &[Check]) -> bool {
    for check in checks {
        if !(check.0)().await {
            return false;
        }
    }
    true
}

async fn readiness(checks: Arc<Vec<Check>>) -> Response {
    match tokio::time::timeout(DEFAULT_TIMEOUT, all_ready(&checks)).await {
        Ok(true) => status(StatusCode::OK, "ready"),
        _ => status(StatusCode::SERVICE_UNAVAILABLE, "not_ready"),
    }
}

fn status(code: StatusCode, value: &str) -> Response {
    json::write(code, &json!({ "status": value }))
}
