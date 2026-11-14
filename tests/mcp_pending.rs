#![cfg(all(feature = "mcptest", feature = "testdb"))]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sqlx::migrate::Migrator;
use time::OffsetDateTime;
use tronc::mcp::{
    Action, GONE, OPEN_LIMIT, OUTCOME_DONE, OUTCOME_RUNNING, PENDING_SCHEMA, Pending, PostgresPending, erase_pending,
    prune_pending,
};
use tronc::mcptest::MemoryPending;
use tronc::testdb;

fn action(id: &str, user_id: &str, age: i64) -> Action {
    let created_at =
        OffsetDateTime::now_utc().replace_nanosecond(0).expect("seconds") - Duration::from_secs(age as u64);
    Action {
        id: id.into(),
        user_id: user_id.into(),
        kind: "k".into(),
        target: id.into(),
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

async fn dedupes_limits_and_erases(store: Arc<dyn Pending>) {
    let first = store.create(&action("d1", "u5", 10)).await.expect("create");
    let again = store.create(&Action { target: "d1".into(), ..action("d2", "u5", 0) }).await.expect("same request");
    assert_eq!((first.id.as_str(), again.id.as_str()), ("d1", "d1"), "the same request reuses the open action");
    let other = Action { target: "d1".into(), digest: "changed".into(), ..action("d3", "u5", 0) };
    assert_eq!(store.create(&other).await.expect("create").id, "d3", "another digest is another action");
    let client_b = Action { target: "d1".into(), client_id: "b".into(), ..action("d4", "u5", 0) };
    assert_eq!(store.create(&client_b).await.expect("create").id, "d4", "another client is another action");
    let again_b = Action { target: "d1".into(), client_id: "b".into(), ..action("d5", "u5", 0) };
    assert_eq!(store.create(&again_b).await.expect("create").id, "d4", "the same client reuses its own action");
    let open: Vec<String> = store.open("u5").await.expect("open").into_iter().map(|a| a.id).collect();
    assert_eq!(open.len(), 3, "one action per digest and client: {open:?}");
    for n in 0..=OPEN_LIMIT {
        store.create(&action(&format!("l{n}"), "u8", 0)).await.expect("create");
    }
    assert_eq!(store.open("u8").await.expect("open").len(), OPEN_LIMIT + 1, "a store may stop at the limit plus one");
    store.create(&action("e1", "u6", 0)).await.expect("create");
    store.erase(&["u5".to_owned()]).await.expect("erase");
    assert!(store.open("u5").await.expect("open").is_empty(), "erase deletes every action");
    assert_eq!(store.open("u6").await.expect("open").len(), 1, "erase spares other people");
}

#[tokio::test]
async fn the_memory_store_claims_once_and_resolves() {
    let store = Arc::new(MemoryPending::new());
    claims_once(store.clone()).await;
    store.claim("u1", "a1").await.expect("claim");
    assert_eq!(store.outcome("a1"), OUTCOME_RUNNING);
    lists_and_resolves(store.clone()).await;
    dedupes_limits_and_erases(store).await;
}

#[tokio::test]
async fn the_postgres_store_claims_once_and_resolves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/mcp");
    let migrator = Migrator::new(dir).await.expect("migrations");
    let Some(pool) = testdb::pool("tronc_test", &migrator).await else { return };
    testdb::truncate(&pool, &migrator, &[]).await.expect("truncate");
    sqlx::raw_sql(PENDING_SCHEMA).execute(&pool).await.expect("the schema is idempotent");
    claims_once(Arc::new(PostgresPending::new(pool.clone()))).await;
    lists_and_resolves(Arc::new(PostgresPending::new(pool.clone()))).await;
    dedupes_limits_and_erases(Arc::new(PostgresPending::new(pool.clone()))).await;
    prunes_closed_actions(pool).await;
}

async fn prunes_closed_actions(pool: sqlx::PgPool) {
    sqlx::query("DELETE FROM mcp_actions").execute(&pool).await.expect("start empty");
    let store = PostgresPending::new(pool.clone());
    store.create(&action("p-open", "u7", 0)).await.expect("create");
    store.create(&action("p-done", "u7", 0)).await.expect("create");
    store.resolve("u7", "p-done", "dismissed").await.expect("resolve");
    let mut stale = action("p-stale", "u7", 7200);
    stale.expires_at = stale.created_at + Duration::from_secs(1);
    store.create(&stale).await.expect("create");
    sqlx::query("UPDATE mcp_actions SET resolved_at = now() - interval '2 hours' WHERE id = 'p-done'")
        .execute(&pool)
        .await
        .expect("age the resolved action");
    assert_eq!(prune_pending(&pool, Duration::from_secs(3600)).await.expect("prune"), 2, "closed and expired go");
    let left: Vec<String> =
        sqlx::query_scalar("SELECT id FROM mcp_actions WHERE user_id = 'u7'").fetch_all(&pool).await.expect("left");
    assert_eq!(left, ["p-open"], "an open action stays");
    erase_pending(&pool, &["u7".to_owned()]).await.expect("erase in a transaction or on the pool");
    assert!(store.open("u7").await.expect("open").is_empty());
}
