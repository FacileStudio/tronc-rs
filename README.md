# tronc-rs

[![ci](https://github.com/FacileStudio/tronc-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/FacileStudio/tronc-rs/actions/workflows/ci.yml)

The Rust app chassis for the [Facile Studio](https://facile.studio) suite: the HTTP behaviour of
Go [`tronc`](https://github.com/FacileStudio/tronc), for the suite apps written in Rust.

Feuilles, Docs and Ardoise have Rust backends. A browser, a CLI or Registre must not be able to
tell them from a Go app, so this crate answers with the same envelope, headers, health routes
and SPA fallback, pinned by test vectors generated from Go `tronc`.

## What it does

- Carries the suite error envelope, `{"error":{"code","message"}}`, with Go's code-to-status
  map, `payment_required` (402) and a safe `upgrade_url`, written byte for byte as Go writes it
- Decodes JSON bodies under a size cap, with Go's field matching, and gzipped bodies separately
- Builds the standard middleware chain as one `tower` layer: request id, security headers, panic
  recovery, client IP, CORS, request log with credential redaction
- Serves `/health` and `/ready` at the root and under `/api`, plus a `healthcheck` argv hook for
  distroless images
- Serves a built SPA with a history fallback that refuses writes, and hashes its inline scripts
  for an enforced CSP
- Applies `sqlx` migrations at boot and serves `migrate up|status|version|baseline`
- Gives tests a real Postgres, one schema per test binary
- Logs JSON lines on stdout and, with `journal`, ships them to Journal
- Serves the `utoipa` OpenAPI 3.1 document behind a Scalar page at `/docs`, and reports
  mounted routes the document forgot
- Calls Bulle as the signed-in user (the Ideas API, or any AI route), with typed plan, status
  and outage errors, and serves the `/api/ideas*` routes to the app's frontend
- Mounts a stateless remote MCP server on rmcp (`mcp::App`, Go's `NewApp`), with the RFC 9728
  metadata, scope gates, confirmations for sensitive tools and the confirm page API
  (`PendingPage`, Go's paths and JSON), plus the `mcptest` conformance kit

## Stack

| Layer | Tech |
|---|---|
| Runtime | Rust 1.98, edition 2024, axum 0.8, tower 0.5, tracing 0.1, utoipa 6 (`apiref`), rmcp 3.9 (`mcp`), reqwest 0.13 on rustls (`bulle`) |
| Storage | PostgreSQL 16 through sqlx 0.9 (features `health`, `migrate`, `testdb`) |

## Install

Pin a tag and pick features. Nothing is on by default.

```toml
[dependencies]
tronc = { git = "https://github.com/FacileStudio/tronc-rs", tag = "v0.5.0", features = ["http", "health", "spa", "migrate", "journal"] }

[dev-dependencies]
tronc = { git = "https://github.com/FacileStudio/tronc-rs", tag = "v0.5.0", features = ["testdb"] }
```

| Feature | Modules |
|---|---|
| always | `errors`, `env`, `logger`, `proxies` |
| `http` | `json`, `http` |
| `health` | `health`, `healthcheck` (pulls in `http`) |
| `spa` | `spa` (pulls in `http`) |
| `migrate` | `migrate` |
| `testdb` | `testdb` (pulls in `migrate`) |
| `journal` | `journal`, and the Journal layer in `logger` |
| `apiref` | `apiref`, the `/docs` reference (pulls in `http`) |
| `bulle` | `bulle`, the Bulle client (typed completions, no redirects) and the `/ideas*` routes (pulls in `http`) |
| `mcp` | `mcp`, the remote MCP server and its Postgres pending store (pulls in `http`) |
| `mcptest` | `mcptest`, the MCP conformance kit and an rmcp test client (pulls in `mcp`) |

A minimal app, the shape of [`examples/minimal.rs`](examples/minimal.rs):

```rust
use axum::Router;
use tronc::{env::Core, health, http, logger, spa};

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let core = Core::load()?;
    let _guard = logger::init(logger::Config::from_core(&core));
    let layers = http::layers(http::Config::from_core(&core))?;
    let router: Router = health::routes(Vec::new())
        .fallback_service(spa::handler(spa::Config { dir: spa::dir_from_env(), ..Default::default() }))
        .layer(layers);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", core.port)).await?;
    let app = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app).await?;
    Ok(())
}
```

Serve with `into_make_service_with_connect_info`: the client-IP rule needs the peer address.

## Documentation

| Doc | What's in it |
|---|---|
| [Architecture](docs/architecture.md) | Middleware order, request lifecycle, where Rust differs from Go |
| [Configuration](docs/configuration.md) | Every environment variable the code reads, and its traps |
| [Development](docs/development.md) | Local setup, the quality gate, vectors, versioning |
| [API](docs/api.md) | Every public item, module by module |
| [Migrations in Rust apps](docs/migrations-rust.md) | Draft addendum to the suite migration standard |

## License

tronc-rs is licensed under the [MIT License](LICENSE), like Go tronc.

---

Part of the [Facile Suite](https://facile.studio) — self-hosted tools for creative studios
and freelancers. One login, zero cloud dependency.
