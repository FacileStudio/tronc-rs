# tronc-rs — Architecture

How a request moves through the chassis, and where the Rust port behaves differently from Go
`tronc` on purpose.

```
Internet ──▶ Traefik ──▶ axum (:PORT) ── tronc::http::layers ──┬──▶ /health, /ready   tronc::health
                                                                ├──▶ /api/*            app modules
                                                                └──▶ fallback          tronc::spa
                                                                          │
                                                                     Postgres 16 (sqlx)
```

## Components

| Module | Port of | Role |
|---|---|---|
| `errors` | `errors`, `httpjson.WriteError` | The envelope, codes, statuses, `write_error` |
| `json` | `httpjson` | Bounded strict decoding, Go-compatible encoding |
| `env` | `env` | `Core` and typed variable helpers |
| `proxies` | `middleware/realip.go`, `cloudflare.go` | Trust sets and address parsing |
| `http` | `httpx`, `middleware` | The chain, CORS, CSP, overrides, rate limit |
| `health`, `healthcheck` | `health`, `healthcheck` | Probes and the argv hook |
| `spa` | `spa` | Static client and inline script hashes |
| `migrate`, `testdb` | `migrate`, `testdb` | sqlx migrations and per-binary test schemas |
| `logger`, `journal` | `logger`, `Journal/sdk/journal` | JSON lines and Journal shipping |
| `apiref` | `apiref` | `/docs` Scalar page, `/docs/openapi.json`, route drift check |
| `mcp` | `mcp` | Stateless MCP endpoint, RFC 9728 metadata, gates, confirmations, paging helpers |
| `mcptest` | `mcp/mcptest` | Conformance kit, rmcp test client, in-memory pending store |

## Request lifecycle

`http::layers` returns one `tower` layer. Outermost first:

1. **Request id.** Keeps an inbound `X-Request-Id` of at most 128 bytes of `[A-Za-z0-9-_.:/]`,
   otherwise mints 26 base32 characters. Echoes it on the response and stores `RequestId`.
2. **Security headers.** Stamps CSP (report-only by default, with framing enforced), X-Frame-
   Options, `nosniff`, Referrer-Policy, Permissions-Policy and COOP on the way out, each only
   when the response lacks it. HSTS goes on when the connection carries the `Tls` extension or a
   trusted peer's rightmost `X-Forwarded-Proto` is `https`. A route's `SecurityOverride` is
   applied here, after the chassis headers.
3. **Panic recovery.** Catches a panic below it and answers the `internal` envelope.
4. **Client IP.** Walks `X-Forwarded-For` right to left from a trusted peer and stores
   `RemoteAddr`: the visitor's bare IP, or the peer's `ip:port` when nothing is believed. The CDN
   header counts only when the rightmost hop is a CDN edge.
5. **CORS.** Mounted only when origins are configured. A refused preflight is a 403.
6. **Request log.** One `http request` event with `kind`, `request_id`, `method`, `path`,
   redacted `query`, `remote_addr`, `client_ip`, `user_agent`, `status`, `bytes` and `duration`
   in nanoseconds. API at info, health and static at debug, any 5xx at error.

## Where Rust differs from Go

| Topic | Go | Rust | Why |
|---|---|---|---|
| Header timing | Set before the handler | Set after, only if absent | tower returns a response; the "handler wins" rule holds |
| Recovery position | Above security headers | Below them | Keeps security headers on a recovered 500 |
| Route override | Mounted under the chassis | Marks the response; the chassis applies it | Same reason |
| Bad config | Panics at construction | `Err` with Go's exact message | An app logs and exits 1 |
| Unknown code | Any string in `Error.Code` | `Code` is an enum; `envelope_response` takes a string | A typo is a compile error |
| TLS | `request.TLS` | `http::Tls` extension | axum has no TLS state |
| Peer address | `RemoteAddr` | `ConnectInfo<SocketAddr>` | Serve with `into_make_service_with_connect_info` |
| Unknown fields | `DisallowUnknownFields` | `#[serde(deny_unknown_fields)]` per type | Checked by `assert_denies_unknown_fields` |
| Response `bytes` | Counted while written | The body's exact size hint, 0 for streams | No writer to wrap |
| SPA conditional GET | `http.ServeContent` | `tower_http::services::ServeFile` | Same 304 and range behaviour |
| Test schema | Kept | Kept | As Go; the plan said drop, Go wins |
| API registry | Hand-written `Registry`, reflected schemas | A `utoipa` `OpenApi`, from `#[utoipa::path]` | The plan chose utoipa; schemas come from derives |
| Route listing | `chi.Walk` | The router's `Debug` output | axum has no route iterator; a test pins the format |

## Data model

The crate owns no table. `migrate` writes sqlx's ledger, `_sqlx_migrations`, in the schema the
pool's `search_path` names, and `testdb::truncate` never empties it.

## Cross-app integration

`journal` posts batches to Journal's `POST /ingest`. `tronc-rs` never depends on `porte-rs`:
auth arrives as extractors the app writes on top of `porte`.
