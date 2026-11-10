# Changelog

All notable changes to `tronc-rs` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow semver: while on
`v0`, a breaking change bumps the minor.

## [Unreleased]

## [0.2.0] - 2026-11-10

### Added

- **`apiref`, the live API reference.** `apiref::routes` serves a Scalar page at `/docs` and the
  `utoipa` OpenAPI 3.1 document at `/docs/openapi.json`, with the CSP widened to the pinned
  bundle's exact URL on that route only, as Go `apiref`. `apiref::undocumented` reports mounted
  routes the document lacks, for the drift test every app keeps; it reads the router's routes
  without running a handler.

## [0.1.0] - 2026-11-10

### Added

- **The chassis core, ported from Go `tronc` v0.20.1.** `errors` (the envelope, Go's
  code-to-status map with `payment_required` and a safe `upgrade_url`, `write_error`), `json`
  (`Json`, `JsonLimit`, `decode`, `decode_gzip`, Go-compatible encoding), `env` (`Core`, `Vars`),
  `proxies`, `logger`, `http` (`layers`: request id, security headers, panic recovery, client IP,
  CORS, request log; plus `SecurityOverride` and `RateLimiter`), `health`, `healthcheck`, `spa`,
  `migrate` on sqlx and `testdb`.
- **`journal`**, a port of `Journal/sdk/journal`: batched `POST /ingest` with retry on 429 and 5xx,
  installed by `logger::init` when `JOURNAL_URL` and `JOURNAL_TOKEN` are set.
- **Conformance.** `tests/conformance.rs` runs all 168 cases of Go `tronc`'s vectors, and
  `scripts/sync-vectors.sh` refreshes them from a tag or a checkout.
- **`examples/minimal.rs`**, driven end to end by `tests/example.rs`.
- **`docs/migrations-rust.md`**, a draft addendum to the suite migration standard.
