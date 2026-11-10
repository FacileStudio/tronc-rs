//! Applies ordered SQL migrations from inside the app binary, as Go `migrate`, on `sqlx`.
//!
//! The app passes `sqlx::migrate!("./migrations")`. `run` applies what is pending at boot under
//! a Postgres advisory lock; `command` serves `migrate up|status|version|baseline <n>` so
//! `docker run <image> migrate status` reaches a distroless container. See
//! `docs/migrations-rust.md` for the file rules.

mod report;

use sqlx::PgPool;
use sqlx::migrate::Migrator;

use crate::errors::{Error, Result};

/// Applies every pending migration in order. A no-op when the schema is current.
pub async fn run(pool: &PgPool, migrator: &Migrator) -> Result<()> {
    let before = report::applied(pool, migrator).await?.len();
    migrator.run(pool).await.map_err(|e| failure("migrate", e))?;
    let after = report::applied(pool, migrator).await?;
    match after.last() {
        Some(version) if after.len() > before => {
            tracing::info!(count = after.len() - before, version = *version, "database migrations applied");
        }
        _ => tracing::debug!("database schema is current"),
    }
    Ok(())
}

/// Runs the `migrate` subcommand when `args[1]` is `migrate`, and returns `None` otherwise so
/// `main` can start normally. Arguments are validated before the database is touched.
pub async fn command(args: &[String], pool: &PgPool, migrator: &Migrator) -> Option<Result<()>> {
    if args.get(1).map(String::as_str) != Some("migrate") {
        return None;
    }
    Some(dispatch(args, pool, migrator).await)
}

/// The subcommand `args` asks for, checked without touching the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subcommand {
    /// Apply pending migrations.
    Up,
    /// Print each migration and whether it is applied. The default.
    Status,
    /// Print the highest applied version.
    Version,
    /// Record migrations up to this version as applied without running them.
    Baseline(i64),
}

/// Parses `migrate [up|status|version|baseline <n>]`.
pub fn parse(args: &[String]) -> Result<Subcommand> {
    match args.get(2).map(String::as_str).unwrap_or("status") {
        "up" => Ok(Subcommand::Up),
        "status" => Ok(Subcommand::Status),
        "version" => Ok(Subcommand::Version),
        "baseline" => {
            let raw = args
                .get(3)
                .ok_or_else(|| Error::invalid("migrate: baseline needs a version, as in 'migrate baseline 1'"))?;
            match raw.parse::<i64>() {
                Ok(version) if version >= 1 => Ok(Subcommand::Baseline(version)),
                _ => Err(Error::invalid(format!("migrate: baseline needs a positive version, got {raw:?}"))),
            }
        }
        other => {
            Err(Error::invalid(format!("migrate: unknown subcommand {other:?}, want up, status, version or baseline")))
        }
    }
}

async fn dispatch(args: &[String], pool: &PgPool, migrator: &Migrator) -> Result<()> {
    match parse(args)? {
        Subcommand::Up => run(pool, migrator).await,
        Subcommand::Status => report::status(pool, migrator).await,
        Subcommand::Version => {
            println!("{}", report::applied(pool, migrator).await?.last().copied().unwrap_or(0));
            Ok(())
        }
        Subcommand::Baseline(target) => baseline(pool, migrator, target).await,
    }
}

async fn baseline(pool: &PgPool, migrator: &Migrator, target: i64) -> Result<()> {
    if let Some(current) = report::applied(pool, migrator).await?.last() {
        return Err(Error::failed(format!(
            "migrate: database is already at version {current}; baseline only stamps a database with no migration history"
        )));
    }
    if !migrator.version_exists(target) {
        return Err(Error::invalid(format!("migrate: no migration has version {target}")));
    }
    migrator.skip(pool, Some(target)).await.map_err(|e| failure("migrate: baseline", e))?;
    let recorded = report::applied(pool, migrator).await?.len();
    tracing::info!(version = target, recorded, "database baselined");
    Ok(())
}

fn failure(context: &str, error: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::internal(format!("{context}: {error}"), error)
}
