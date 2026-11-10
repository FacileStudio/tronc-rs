# tronc-rs — Migrations in Rust apps

A draft addendum to the suite migration standard (`standards/migrations.md`), which covers goose
and Go only. **Status: proposal, not ratified.** The owner reviews it and, once accepted, moves it
into the standard; until then the standard is unchanged and this page binds nothing.

## Decision

Rust apps (Feuilles, Docs, Ardoise) migrate with **sqlx's migrator, run through
`tronc::migrate`**. It is the Rust counterpart of `tronc/migrate`: SQL files embedded in the
binary, applied at boot behind a Postgres advisory lock, with a `migrate` subcommand for the
operator. goose is not used, because no Rust binding embeds it, and the apps already depend on
sqlx for queries.

## Wiring

```rust
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

if let Some(result) = tronc::migrate::command(&args, &pool, &MIGRATOR).await {
    return if result.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE };
}
tronc::migrate::run(&pool, &MIGRATOR).await?;
```

- `sqlx::migrate!` needs sqlx's `macros` and `migrate` features. It reads the files at compile
  time; the `query!` macros stay banned (they need a database at build time).
- Add a `build.rs` with `println!("cargo:rerun-if-changed=migrations");`. Without it, adding a
  file does not rebuild the binary and the new migration silently never ships.
- `docker run <image> migrate status|up|version|baseline <n>` works on the entrypoint-only
  image, as in Go.

## Files

- **Name:** `migrations/NNNN_snake_name.sql`, four digits, sequential: `0001_baseline.sql`,
  `0002_add_sheet_owner.sql`. sqlx reads the integer before the first `_` as the version.
- **Forward only.** No `.down.sql`. Roll forward, as the standard already says for goose.
- **Never edit a released file.** sqlx stores a checksum per version and refuses to boot with
  `migration N was previously applied but has been modified`. That is stricter than goose,
  which would replay nothing and say nothing.
- **No annotations.** A file is sent as one multi-statement query, so `DO $$ … $$` blocks and
  function bodies need no `StatementBegin` wrapper.

## Transactions

Each file runs in one transaction. A file whose first line is `-- no-transaction` runs outside
one, which `CREATE INDEX CONCURRENTLY` needs. The goose rule carries over: **one statement per
no-transaction file**, because a partial failure leaves applied statements and no ledger row.
Split `ALTER TYPE … ADD VALUE` and its first use into two files.

## Ledger and locking

- The ledger is `_sqlx_migrations`, in the first schema of the pool's `search_path`. A failed
  migration leaves a row with `success = false`; sqlx then refuses to run until an operator
  fixes the database and deletes that row.
- sqlx takes `pg_advisory_lock` with an id derived from the database name. Every schema in one
  database shares it, so parallel test binaries queue rather than collide. Do not cap the pool
  at one connection, for the same reason as goose.

## Baselining

`migrate baseline <n>` records versions up to `n` as applied without running them, for a
database whose schema already exists. It refuses a database that has any history, and a version
no file has. The Ardoise port needs it: its schema is Prisma's, and the Rust binary must adopt it
without replaying it.

## Tests

`tronc::testdb::pool(prefix, &MIGRATOR)` creates `<prefix>_<test binary>` and migrates it, so
every test runs against the real migrations. `testdb::truncate` keeps the ledger.

## Boot time

The goose rule holds unchanged: a compose `healthcheck.start_period` must exceed the worst-case
migration time, or Dokploy kills the container mid-migration. Run a long one beforehand with
`docker run <image> migrate up`.

## What stays the same

Backups before touching la ruche, baselining from production rather than from code, and the
verification diff all apply to Rust apps exactly as written in the standard.
