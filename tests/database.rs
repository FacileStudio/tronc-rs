#![cfg(feature = "testdb")]

use std::path::Path;

use sqlx::migrate::Migrator;
use tronc::{migrate, testdb};

async fn migrator() -> Migrator {
    Migrator::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/migrations")).await.expect("migrations")
}

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

#[test]
fn subcommands_are_validated_before_the_database() {
    assert_eq!(migrate::parse(&args(&["api", "migrate"])).ok(), Some(migrate::Subcommand::Status));
    assert_eq!(
        migrate::parse(&args(&["api", "migrate", "baseline", "7"])).ok(),
        Some(migrate::Subcommand::Baseline(7))
    );
    assert!(
        migrate::parse(&args(&["api", "migrate", "baseline"])).is_err_and(|e| e.message.contains("needs a version"))
    );
    assert!(migrate::parse(&args(&["api", "migrate", "baseline", "0"])).is_err_and(|e| e.message.contains("positive")));
    assert!(
        migrate::parse(&args(&["api", "migrate", "sideways"])).is_err_and(|e| e.message.contains("unknown subcommand"))
    );
}

#[tokio::test]
async fn testdb_migrates_its_own_schema_and_truncates_keeping_the_ledger() {
    let migrator = migrator().await;
    let Some(pool) = testdb::pool("tronc_test", &migrator).await else { return };
    let schema: String = sqlx::query_scalar("SELECT current_schema()::text").fetch_one(&pool).await.expect("schema");
    assert_eq!(schema, testdb::schema_name("tronc_test"));
    assert_eq!(schema, "tronc_test_database");
    sqlx::query("INSERT INTO sheets (name, owner) VALUES ('a', 'b')").execute(&pool).await.expect("insert");
    testdb::truncate(&pool, &migrator, &[]).await.expect("truncate");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM sheets").fetch_one(&pool).await.expect("count");
    assert_eq!(rows, 0);
    assert!(migrate::command(&args(&["api", "migrate", "version"]), &pool, &migrator).await.expect("handled").is_ok());
    assert!(migrate::command(&args(&["api", "serve"]), &pool, &migrator).await.is_none());
}

async fn fresh_schema(name: &str) -> Option<sqlx::PgPool> {
    let url = testdb::url()?;
    let admin = sqlx::PgPool::connect(&url).await.expect("connect");
    let reset = format!("DROP SCHEMA IF EXISTS {name} CASCADE; CREATE SCHEMA {name}");
    sqlx::raw_sql(sqlx::AssertSqlSafe(reset)).execute(&admin).await.expect("schema");
    let options: sqlx::postgres::PgConnectOptions = url.parse().expect("url");
    Some(sqlx::PgPool::connect_with(options.options([("search_path", name)])).await.expect("scoped"))
}

#[tokio::test]
async fn baseline_stamps_an_existing_schema_then_up_applies_the_rest() {
    let migrator = migrator().await;
    let Some(pool) = fresh_schema("tronc_test_baseline").await else { return };
    sqlx::query("CREATE TABLE sheets (id bigserial PRIMARY KEY, name text NOT NULL)")
        .execute(&pool)
        .await
        .expect("legacy");
    let run = |words: &'static [&'static str]| {
        let (pool, migrator) = (pool.clone(), &migrator);
        async move { migrate::command(&args(words), &pool, migrator).await.expect("handled") }
    };
    run(&["api", "migrate", "baseline", "1"]).await.expect("baseline");
    assert!(run(&["api", "migrate", "baseline", "1"]).await.is_err_and(|e| e.message.contains("already at version 1")));
    run(&["api", "migrate", "up"]).await.expect("up");
    let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&pool)
        .await
        .expect("ledger");
    assert_eq!(versions, [1, 2]);
    sqlx::query("INSERT INTO sheets (name, owner) VALUES ('a', 'b')").execute(&pool).await.expect("owner column");
    run(&["api", "migrate", "status"]).await.expect("status");
}

#[tokio::test]
async fn baseline_refuses_a_version_no_file_has() {
    let migrator = migrator().await;
    let Some(pool) = fresh_schema("tronc_test_baseline_missing").await else { return };
    let result =
        migrate::command(&args(&["api", "migrate", "baseline", "9"]), &pool, &migrator).await.expect("handled");
    assert!(result.is_err_and(|e| e.message.contains("no migration has version 9")));
}
