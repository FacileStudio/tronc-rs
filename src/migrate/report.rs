use std::ops::DerefMut;

use sqlx::migrate::{Migrate, Migrator};
use sqlx::{AssertSqlSafe, PgPool, Row};

use super::failure;
use crate::errors::Result;

pub async fn applied(pool: &PgPool, migrator: &Migrator) -> Result<Vec<i64>> {
    let mut conn = pool.acquire().await.map_err(|e| failure("migrate", e))?;
    conn.deref_mut().ensure_migrations_table(&migrator.table_name).await.map_err(|e| failure("migrate", e))?;
    let applied =
        conn.deref_mut().list_applied_migrations(&migrator.table_name).await.map_err(|e| failure("migrate", e))?;
    Ok(applied.into_iter().map(|migration| migration.version).collect())
}

pub async fn status(pool: &PgPool, migrator: &Migrator) -> Result<()> {
    applied(pool, migrator).await?;
    let query = format!("SELECT version, installed_on::text AS at FROM {} WHERE success", migrator.table_name);
    let rows = sqlx::query(AssertSqlSafe(query)).fetch_all(pool).await.map_err(|e| failure("migrate", e))?;
    let installed: Vec<(i64, String)> = rows.iter().map(|row| (row.get("version"), row.get("at"))).collect();
    for migration in migrator.iter().filter(|m| m.migration_type.is_up_migration()) {
        let at = installed.iter().find(|(version, _)| *version == migration.version).map(|(_, at)| at.as_str());
        let state = if at.is_some() { "applied" } else { "pending" };
        println!("{:<6} {:<8} {:<30} {}", migration.version, state, at.unwrap_or("-"), migration.description);
    }
    Ok(())
}
