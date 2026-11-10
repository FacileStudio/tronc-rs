//! Connects a test to a real Postgres, as Go `testdb`: one schema per test binary, reached
//! through `search_path`, migrated with the app's own migrator.
//!
//! ```ignore
//! let Some(pool) = tronc::testdb::pool("feuilles_test", &sqlx::migrate!("./migrations")).await else {
//!     return;
//! };
//! ```
//!
//! An unset `TEST_DATABASE_URL` skips with a banner instead of failing, so a machine without
//! Postgres still runs the rest of the gate. The schema is kept between runs, as in Go; call
//! [`truncate`] to reset its rows.

use std::str::FromStr;
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{Executor, PgPool};

/// How long a connection attempt may take, so a blackholed host fails fast in a hook.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// The variable naming the scratch database.
pub const ENV_VAR: &str = "TEST_DATABASE_URL";

/// The connection string, or `None` when the database-backed tests should skip.
pub fn url() -> Option<String> {
    std::env::var(ENV_VAR).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty())
}

/// The skip message, naming the command that turns the tests on.
pub fn skip_reason(bootstrap: &str) -> String {
    let bootstrap =
        if bootstrap.is_empty() { "# start a PostgreSQL and create a scratch database, then:" } else { bootstrap };
    format!(
        "{ENV_VAR} is not set, so the database-backed tests did not run.\n\nPoint them at a scratch database:\n\n    {bootstrap}\n    export {ENV_VAR}='postgres://localhost:5433/scratch?sslmode=disable'"
    )
}

/// The schema this test binary owns: `<prefix>_<binary name>`, with Cargo's hash suffix cut.
pub fn schema_name(prefix: &str) -> String {
    let exe = std::env::current_exe().ok().and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()));
    let exe = exe.unwrap_or_default();
    let base = match exe.rsplit_once('-') {
        Some((name, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => name.to_owned(),
        _ => exe,
    };
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                c
            } else if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("{prefix}_{}", if safe.is_empty() { "anonymous" } else { &safe })
}

/// Creates this binary's schema, connects scoped to it and migrates. `None`, after a banner on
/// stderr, when `TEST_DATABASE_URL` is unset. Panics when the database cannot be reached or
/// migrated: a test helper has nobody to hand the error to.
pub async fn pool(prefix: &str, migrator: &Migrator) -> Option<PgPool> {
    let Some(url) = url() else {
        eprintln!("\n=== SKIPPED: {ENV_VAR} is not set ===\n\n{}\n", skip_reason(""));
        return None;
    };
    let schema = schema_name(prefix);
    let options = PgConnectOptions::from_str(&url).unwrap_or_else(|e| panic!("testdb: {ENV_VAR} does not parse: {e}"));
    let admin = PgPoolOptions::new().max_connections(1).acquire_timeout(TIMEOUT).connect_with(options.clone()).await;
    let admin = admin.unwrap_or_else(|e| panic!("testdb: open {ENV_VAR}: {e}"));
    let create = format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"");
    if let Err(error) = admin.execute(sqlx::AssertSqlSafe(create)).await {
        let raced =
            error.as_database_error().and_then(|e| e.code()).is_some_and(|code| code == "23505" || code == "42P06");
        assert!(raced, "testdb: create schema {schema}: {error}");
    }
    admin.close().await;
    let scoped = options.options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new().max_connections(5).acquire_timeout(TIMEOUT).connect_with(scoped).await;
    let pool = pool.unwrap_or_else(|e| panic!("testdb: open {ENV_VAR} scoped to {schema}: {e}"));
    migrator.run(&pool).await.unwrap_or_else(|e| panic!("testdb: migrate {schema}: {e}"));
    Some(pool)
}

/// Empties every table in the current schema except the migration ledger and `keep`, and
/// restarts identity sequences.
pub async fn truncate(pool: &PgPool, migrator: &Migrator, keep: &[&str]) -> sqlx::Result<()> {
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT tablename::text FROM pg_tables WHERE schemaname = current_schema()")
            .fetch_all(pool)
            .await?;
    let quoted: Vec<String> = tables
        .iter()
        .filter(|table| table.as_str() != migrator.table_name && !keep.contains(&table.as_str()))
        .map(|table| format!("\"{}\"", table.replace('"', "\"\"")))
        .collect();
    if quoted.is_empty() {
        return Ok(());
    }
    let statement = format!("TRUNCATE TABLE {} RESTART IDENTITY CASCADE", quoted.join(", "));
    pool.execute(sqlx::AssertSqlSafe(statement)).await.map(|_| ())
}
