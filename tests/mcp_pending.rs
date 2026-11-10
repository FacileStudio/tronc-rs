#![cfg(all(feature = "mcptest", feature = "testdb"))]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sqlx::migrate::Migrator;
use time::OffsetDateTime;
use tronc::mcp::{Action, GONE, OUTCOME_DONE, OUTCOME_RUNNING, PENDING_SCHEMA, Pending, PostgresPending};
use tronc::mcptest::MemoryPending;
use tronc::testdb;

fn action(id: &str, user_id: &str, age: i64) -> Action {
    let created_at =
        OffsetDateTime::now_utc().replace_nanosecond(0).expect("seconds") - Duration::from_secs(age as u64);
    Action {
        id: id.into(),
        user_id: user_id.into(),
        kind: "k".into(),
        target: "t".into(),
        digest: "d".into(),
        summary: "Delete?".into(),
        client_id: "c".into(),
        created_at,
        expires_at: created_at + Duration::from_secs(3600),
    }
}

async fn claims_once(store: Arc<dyn Pending>) {
    store.create(&action("a1", "u1", 0)).await.expect("create");
    assert_eq!(store.claim("u2", "a1").await.expect_err("stranger").message, GONE, "another person must not claim");
    let racers: Vec<_> = (0..20)
        .map(|_| {
            let store = store.clone();
            tokio::spawn(async move { store.claim("u1", "a1").await.is_ok() })
        })
        .collect();
    let mut wins = 0;
    for racer in racers {
        wins += usize::from(racer.await.expect("join"));
    }
    assert_eq!(wins, 1, "exactly one racing claim wins");
    store.release("a1").await.expect("release");
    assert_eq!(store.open("u1").await.expect("open").len(), 1, "a released action is open again");
}

async fn lists_and_resolves(store: Arc<dyn Pending>) {
    store.create(&action("old", "u3", 60)).await.expect("create");
    store.create(&action("new", "u3", 0)).await.expect("create");
    let mut expired = action("gone", "u3", 7200);
    expired.expires_at = expired.created_at + Duration::from_secs(1);
    store.create(&expired).await.expect("create");
    let open: Vec<String> = store.open("u3").await.expect("open").into_iter().map(|a| a.id).collect();
    assert_eq!(open, ["new", "old"], "open actions, newest first, the expired one left out");
    let claimed = store.claim("u3", "new").await.expect("claim");
    let expected = Action { created_at: claimed.created_at, expires_at: claimed.expires_at, ..action("new", "u3", 0) };
    assert_eq!(claimed, expected, "a claim returns every column, the digest included");
    store.finish("new", OUTCOME_DONE).await.expect("finish");
    store.release("new").await.expect("release is a no-op once finished");
    assert_eq!(store.claim("u3", "new").await.expect_err("finished").message, GONE);
    assert_eq!(store.resolve("u4", "old", "dismissed").await.expect_err("stranger").message, GONE);
    store.resolve("u3", "old", "dismissed").await.expect("resolve");
    assert_eq!(store.resolve("u3", "old", "dismissed").await.expect_err("twice").message, GONE);
}

#[tokio::test]
async fn the_memory_store_claims_once_and_resolves() {
    let store = Arc::new(MemoryPending::new());
    claims_once(store.clone()).await;
    store.claim("u1", "a1").await.expect("claim");
    assert_eq!(store.outcome("a1"), OUTCOME_RUNNING);
    lists_and_resolves(store).await;
}

#[tokio::test]
async fn the_postgres_store_claims_once_and_resolves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/mcp");
    let migrator = Migrator::new(dir).await.expect("migrations");
    let Some(pool) = testdb::pool("tronc_test", &migrator).await else { return };
    testdb::truncate(&pool, &migrator, &[]).await.expect("truncate");
    sqlx::raw_sql(PENDING_SCHEMA).execute(&pool).await.expect("the schema is idempotent");
    claims_once(Arc::new(PostgresPending::new(pool.clone()))).await;
    lists_and_resolves(Arc::new(PostgresPending::new(pool))).await;
}
