# tronc-rs — Configuration

Every environment variable the crate reads, where it reads it, and the traps.

`env::Core::load()` reads the shared set. A missing or malformed required variable is an
`EnvError` such as `env: DATABASE_URL is required`; the app logs it and exits 1, as a Go app does.

| Variable | Required | Default | Read by | What it does |
|---|---|---|---|---|
| `DATABASE_URL` | yes, unless `Core::load_without_database` | — | `env::Core` | Postgres connection string |
| `PORT` | no | `8080` | `env::Core`, `healthcheck` | Listen port, and the port the probe targets |
| `APP_ENV` | no | `development` | `env::Core` | `production`/`prod`, `staging`/`stage`, anything else is development |
| `LOG_LEVEL` | no | `info` | `env::Core`, `logger` | `debug`, `info`, `warn`/`warning`, `error`; anything else is info |
| `CORS_ALLOWED_ORIGINS` | no | none | `env::Core` | Comma-separated origins; empty mounts no CORS layer |
| `ALLOWED_ORIGINS`, `DOMAINS`, `DOMAIN`, `CORS_ORIGINS`, `TRUSTED_ORIGINS`, `CLIENT_ORIGIN` | no | none | `env::Core` | Older names for the same list, read in this order when the first is unset |
| `TRUSTED_PROXIES` | no | `private` | `env::Core` | CIDR blocks, addresses, `private`, `cloudflare`, or `none` |
| `JOURNAL_URL` | no | none | `logger::Config::from_core` | Journal API base URL |
| `JOURNAL_TOKEN` | no | none | `logger::Config::from_core` | Journal ingest key; shipping starts only when both are set |
| `CLIENT_DIR` | no | `./client` | `spa::dir_from_env` | The built SPA |
| `TEST_DATABASE_URL` | for database tests | none | `testdb` | Scratch Postgres; unset skips with a banner |

## `TRUSTED_PROXIES`

- Unset means `private`: loopback, RFC 1918, link-local and the IPv6 private ranges. That is the
  peer every app sees behind Traefik.
- `none` trusts nothing; the connection address is the client. An empty value is unset, not
  `none`, so a half-written config never picks the strictest setting silently.
- `cloudflare` adds Cloudflare's edge ranges and turns on `Cf-Connecting-Ip`, believed only when
  the rightmost `X-Forwarded-For` hop is a Cloudflare address.
- Names ignore case and surrounding spaces. Anything else must parse as a CIDR block or an IP:
  `private,not-an-ip` is refused with `env: TRUSTED_PROXIES takes CIDR blocks, IP addresses, or
  the names private and cloudflare: invalid CIDR address or IP: not-an-ip`.

## Traps

- **`PORT` defaults to 8080, not 4000.** Go `tronc` defaults to 8080 in both `env` and
  `healthcheck`, and this crate keeps that. An app whose docs say 4000 must set `PORT=4000` in
  its image, or the healthcheck probes the wrong port.
- **Serve with connect info.** Without `into_make_service_with_connect_info::<SocketAddr>()`
  the chain sees no peer: no forwarded header is believed and HSTS is never sent from a proxy.
- **`APP_ENV` gates nothing.** CORS is the origin list alone.
- **CORS `*` with credentials** is refused by `http::layers`, as Go panics on it.

## Typed helpers

`env::Vars` reads the process (`Vars::process()`) or a fixed map (`Vars::from_pairs`), so tests
never mutate the process environment. `string`, `required`, `int`, `bool` (Go's
`strconv.ParseBool` spellings), `duration` (Go syntax: `300ms`, `1.5h`, `2h45m`) and `list`
(comma-separated, blanks dropped) all trim the value and treat blank as unset.
