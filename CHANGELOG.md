# Changelog

All notable changes to `tronc-rs` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow semver: while on
`v0`, a breaking change bumps the minor.

## [Unreleased]

## [0.5.0] - 2026-11-14

Catches up with Go tronc v0.22 and v0.23.0, so Toile and Docs can drop their own confirm page,
MCP wiring, erase query and Bulle plumbing. Go equivalents are named in each entry.

### Added

- **`mcp::App`, the Rust `NewApp`.** `App::new(AppConfig, AppSpec)` builds the server options
  and the confirmations together: the confirm link at the resource's origin plus `/pending`,
  `AppSpec::scopes` for read-only servers, and 120 calls a minute per person with a burst of 120
  (`DEFAULT_PER_MINUTE`). Tools that ask before acting close over `app.confirmations`, then
  `app.mount(router)`.
- **`mcp::PendingPage`, the confirm page API on Go's paths and JSON** (`PendingPage`,
  `PendingAPIPath`). `GET /mcp/pending` answers `{"actions":[...]}` plus `"truncated":true`
  past 100 actions, confirm answers `{"status":"done"}` or the app's `done` body, dismiss
  answers 200 `{"status":"dismissed"}`. The `actor` (such as `docs-web`) marks page
  confirmations in the audit. The `user` hook refuses anything but the person's own session
  with `delegated()` (Go `ErrDelegated`); `describe` adds the app's view of each target.
  `PENDING_ROUTES` documents the three routes.
- **Confirmation texts.** `Outcome::message` carries `Confirmations::waiting()` ("Nothing
  happened yet...", with the pending lifetime) or `MESSAGE_DECLINED`, as Go v0.23.0.
  `Confirmations::ask` and `answered` wrap the request/resume skeleton every sensitive tool
  copied and answer a `Confirmation` (`status`, `confirm_url`, `message`).
- **Pending store upkeep.** `Confirmations::request` reuses the person's open action of the same
  kind, target and digest, so an agent asking twice leaves one entry. `Confirmations::open`
  lists at most `OPEN_LIMIT` (100) and `list` also reports `truncated`. `Pending::erase` and
  `erase_pending(executor, ids)` (Go `ErasePending`) delete a person's actions inside an erasure
  transaction. `Pruner`, `PostgresPending::prune`, `prune_pending` and `keep_pruned` (Go
  `Prune`, `PrunePending`, `KeepPruned`) delete actions resolved or expired more than 30 days
  ago, every hour by default.
- **Go helpers:** `digest` (`Digest`), `target_ids` (`TargetIDs`), `state_key` (`StateKey`,
  sha256 of `<app>-mcp-state:` and the secret), `EXTRA_SUBJECT` (porte `mcpauth.ExtraSubject`).
- **Paging:** `page(items, cursor, size)`, `offset` and `cursor` (Go `Page`, `Offset`,
  `Cursor`), with `fit_count` guidance for results near `RESULT_BUDGET`.
- **Annotation presets** `Annotations::READ`, `ADD`, `SET`, `OVERWRITE` and `REMOVE`, encoding
  the suite rule: overwriting or removing user data, or silencing an alert, is destructive;
  only a repeat with no further effect is idempotent, so `REMOVE`, which asks for a
  confirmation, is not.
- **`failure` parity with Go v0.23.0:** an `unavailable` error keeps its message, an unsafe
  upgrade link is dropped, and `PlanLimit` plus `plan_failure` turn an outside plan refusal
  (porte's forfait limit) into "The plan limit for X is reached. Upgrade at ...".
- **`Error::with_key`.** The envelope carries `key` (such as `ai.access`), which muse's
  `planLimitFrom` reads; a Bulle 402 keeps its key through `From<bulle::Error>`.
- **Rate limit parity with Go v0.21:** `RateLimitConfig::max_keys` bounds the buckets (the MCP
  server keeps 10,000), `on_refuse` writes the refusal, and `remote_addr_key64` keys IPv6
  callers on their /64. The MCP limiter keys a caller without a user on that /64 too.
- **Bulle, as Go v0.23.0:** `Client::complete(CompleteRequest)` with an optional `space_id`
  that is only sent when set, `Client::decide_ai(DecideRequest)` with the
  `X-Bulle-Acting-For` header (`DecideAI`), `PlanRequired::limit` from the 402 `limit` field,
  `Request::header`, and `Client::with_timeout`.
- **Conformance:** runners for the v0.23.0 vectors `mcp_http`, `mcp_rate_limit`, `mcp_digest`,
  `mcp_state_key`, `mcp_failure`, `bulle_errors` and `bulle_paths`.

### Changed

- **Breaking:** `Pending::create` returns the stored `Action` and reuses an open duplicate,
  `Pending::open` may stop after 101 actions, and the trait gains `erase`. `Outcome` gains
  `message`.
- **Breaking:** `tronc::Error` gains a `key` field; build errors with the constructors.
- **Breaking:** `bulle::Error` gains the `NoActingFor` variant, so an exhaustive `match` needs a
  new arm.
- **Breaking:** `bulle::PlanRequired` gains `limit` (end a literal with `..Default::default()`)
  and `bulle::Request` gains `headers` (build it with `Request::new`).
- **Breaking:** `http::RateLimitConfig` gains `max_keys` and `on_refuse`; end a literal with
  `..RateLimitConfig::default()`.
- `Confirmations::open` returns at most 100 actions (`OPEN_LIMIT`), where it returned all.
- The MCP rate limiter keeps at most 10,000 keys and keys a caller without a user on its IPv6
  /64, where it kept every key and used the full address.
- `bulle::Client::new` never follows a redirect, so a user's token never reaches another host.
  Build one client at boot and clone it to reuse its connection pool.
- CI re-syncs the conformance vectors from the Go tag in `tests/vectors/SOURCE` (v0.23.0) and
  fails on any difference, new vector files included. `SOURCE` now holds only the tag; each
  file keeps the `source` of the release that last changed it.

## [0.4.0] - 2026-11-11

### Added

- **`bulle`, the Bulle client, ported from Go `tronc/bulle` (v0.18.0).** Every AI feature in the
  suite goes through Bulle and needs a plan with `ai.access`. `Client` covers the Ideas API
  (`ideas`, `generate`, `run`, `run_status`, `decide`, `dismiss`) plus `call` for any later AI
  route, with paths built from escaped segments, a 20 s default timeout and a 1 MiB answer cap.
  The token comes from a `TokenSource` the caller supplies (a `&str` works), so porte-rs's token
  exchange plugs in without tronc depending on it; `forget` retries a 401 once. A 402 is
  `Error::PlanRequired` (bare body or envelope, unsafe `upgrade_url` dropped), other 4xx keep
  their status in `Error::Status`, and 5xx, timeouts and network errors are
  `Error::Unreachable`. `From<bulle::Error> for tronc::Error` maps them for the frontend as Go
  does. `mount_ideas` serves `/ideas*` with the plan pre-check (`ai_access`) and the active
  space supplied by the app. New dependency: reqwest 0.13 with rustls, behind the feature.

## [0.3.0] - 2026-11-10

### Added

- **`mcp`, the remote MCP server, ported from Go `tronc/mcp`.** rmcp 3.9's Streamable HTTP
  transport in stateless mode (spec 2026-07-28): no session, any replica answers. `Server` and
  `mount` serve the endpoint at the resource's path and the RFC 9728 metadata at both well-known
  paths, behind an Origin check, a bearer token from the app's `Verifier` (RFC 6750 challenges
  with `invalid_token` and `insufficient_scope`), a write-scope gate that reads the JSON-RPC body
  and batches, and a per-person rate limit (2/s, burst 30). `tool` builds a typed tool whose
  annotations are mandatory; names carry the app prefix; `tools/list` is sorted and cacheable
  privately. `Confirmations` asks before a sensitive action: MRTR `input_required` when the
  client can elicit, otherwise a pending action and a confirm link, with HMAC-signed state,
  single-use claims and a changed-target guard, on `PostgresPending` and the
  `migrations/mcp/0001_mcp_actions.sql` table. `failure`, `clip`, `fit_count` and the cursor
  helpers keep results actionable and under about 25k tokens.
- **`mcptest`**, the conformance kit: `run`, an rmcp test client, `MemoryPending`.
- **`examples/mcp.rs`**, driven by an rmcp client in `tests/mcp_example.rs`.

### Changed

- `http::plain` takes any body, not only a `&'static str`.

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
