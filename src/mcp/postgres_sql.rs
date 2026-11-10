use sqlx::postgres::PgRow;
use sqlx::{PgPool, Row};

use super::pending::{Action, OUTCOME_RUNNING, gone};
use crate::errors::{Error, Result};

const COLUMNS: &str = "id, user_id, kind, target, digest, summary, client_id, created_at, expires_at";

fn scan(row: &PgRow) -> std::result::Result<Action, sqlx::Error> {
    Ok(Action {
        id: row.try_get("id")?,
        user_id: row.try_get("user_id")?,
        kind: row.try_get("kind")?,
        target: row.try_get("target")?,
        digest: row.try_get("digest")?,
        summary: row.try_get("summary")?,
        client_id: row.try_get("client_id")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

fn failed(what: &str) -> impl FnOnce(sqlx::Error) -> Error {
    let message = format!("failed to {what} a pending action");
    move |error| Error::internal(message, error)
}

pub(super) async fn claim(pool: &PgPool, user_id: &str, id: &str) -> Result<Action> {
    let sql = format!(
        "UPDATE mcp_actions SET resolved_at = now(), outcome = $3 \
         WHERE id = $1 AND user_id = $2 AND resolved_at IS NULL AND expires_at > now() RETURNING {COLUMNS}"
    );
    let query = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).bind(user_id).bind(OUTCOME_RUNNING);
    let row = query.fetch_optional(pool).await.map_err(failed("read"))?;
    row.as_ref().map(scan).transpose().map_err(failed("read"))?.ok_or_else(gone)
}

pub(super) async fn open(pool: &PgPool, user_id: &str) -> Result<Vec<Action>> {
    let sql = format!(
        "SELECT {COLUMNS} FROM mcp_actions WHERE user_id = $1 AND resolved_at IS NULL AND expires_at > now() \
         ORDER BY created_at DESC, id"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(user_id).fetch_all(pool).await.map_err(failed("list"))?;
    rows.iter().map(scan).collect::<std::result::Result<_, _>>().map_err(failed("list"))
}

pub(super) async fn create(pool: &PgPool, a: &Action) -> Result<()> {
    let sql = format!("INSERT INTO mcp_actions ({COLUMNS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)");
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(&a.id)
        .bind(&a.user_id)
        .bind(&a.kind)
        .bind(&a.target)
        .bind(&a.digest)
        .bind(&a.summary)
        .bind(&a.client_id)
        .bind(a.created_at)
        .bind(a.expires_at)
        .execute(pool)
        .await
        .map(drop)
        .map_err(failed("create"))
}

pub(super) async fn resolve(pool: &PgPool, user_id: &str, id: &str, outcome: &str) -> Result<()> {
    let sql = "UPDATE mcp_actions SET resolved_at = now(), outcome = $3 WHERE id = $1 AND user_id = $2 AND resolved_at IS NULL";
    let done = sqlx::query(sql).bind(id).bind(user_id).bind(outcome).execute(pool).await.map_err(failed("resolve"))?;
    if done.rows_affected() == 0 { Err(gone()) } else { Ok(()) }
}

pub(super) async fn finish(pool: &PgPool, id: &str, outcome: &str) -> Result<()> {
    let query = sqlx::query("UPDATE mcp_actions SET outcome = $2 WHERE id = $1").bind(id).bind(outcome);
    query.execute(pool).await.map(drop).map_err(failed("finish"))
}

pub(super) async fn release(pool: &PgPool, id: &str) -> Result<()> {
    let sql = "UPDATE mcp_actions SET resolved_at = NULL, outcome = '' WHERE id = $1 AND outcome = $2";
    let query = sqlx::query(sql).bind(id).bind(OUTCOME_RUNNING);
    query.execute(pool).await.map(drop).map_err(failed("release"))
}
