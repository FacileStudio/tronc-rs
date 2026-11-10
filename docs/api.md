# tronc-rs — API

Every public item, module by module, with the feature that turns it on. `cargo doc
--all-features --open` renders the same surface with full doc comments. The crate serves no
HTTP route of its own beyond the ones listed under `health` and `spa`, so there is no live
`/docs` reference here; apps mount theirs.

## Crate root (always)

```rust
pub use errors::{Code, Error, Result};
```

## `errors` (always; responses need `http`)

```rust
pub enum Code { InvalidArgument, Unauthenticated, PaymentRequired, PermissionDenied, NotFound,
    AlreadyExists, FailedPrecondition, ResourceExhausted, RateLimited, MethodNotAllowed,
    Unavailable, Internal }
impl Code {
    pub const fn as_str(self) -> &'static str;
    pub const fn status(self) -> http::StatusCode;
    pub fn parse(value: &str) -> Option<Code>;
}
pub fn status_for(code: &str) -> http::StatusCode;
pub type Cause = Box<dyn std::error::Error + Send + Sync>;
pub struct Error { pub code: Code, pub message: String, pub upgrade_url: Option<String>, pub cause: Option<Cause> }
impl Error {
    pub fn new(code: Code, message: impl Into<String>) -> Self;
    pub fn invalid / unauthorized / payment_required / forbidden / not_found / conflict / failed
        / too_large / rate_limited / not_allowed / unavailable (message: impl Into<String>) -> Self;
    pub fn internal(message: impl Into<String>, cause: impl Into<Cause>) -> Self;
    pub fn with_upgrade_url(self, raw: impl Into<String>) -> Self;
}
impl axum::response::IntoResponse for Error;
pub fn safe_upgrade_url(raw: &str) -> bool;
pub fn envelope(code: &str, message: &str, upgrade_url: Option<&str>) -> serde_json::Value;
pub fn envelope_response(code: &str, message: &str, upgrade_url: Option<&str>) -> Response;
pub fn write_error(error: &(dyn std::error::Error + 'static)) -> Response;
pub type Result<T> = std::result::Result<T, Error>;
```

| Code | Status | Constructor |
|---|---|---|
| `invalid_argument` | 400 | `invalid` |
| `unauthenticated` | 401 | `unauthorized` |
| `payment_required` | 402 | `payment_required` |
| `permission_denied` | 403 | `forbidden` |
| `not_found` | 404 | `not_found` |
| `method_not_allowed` | 405 | `not_allowed` |
| `already_exists` | 409 | `conflict` |
| `failed_precondition` | 412 | `failed` |
| `resource_exhausted` | 413 | `too_large` |
| `rate_limited` | 429 | `rate_limited` |
| `internal` | 500 | `internal` |
| `unavailable` | 503 | `unavailable` |

An `Internal` error logs its cause at error level and sends the message only.

## `json` (`http`)

```rust
pub const MAX_BODY_BYTES: usize = 1 << 20;
pub struct Json<T>(pub T);                         // extractor + 200 response
pub struct JsonLimit<T, const LIMIT: usize>(pub T); // extractor with a per-route cap
pub fn decode<T: DeserializeOwned>(body: &[u8], limit: usize) -> Result<T>;
pub fn decode_gzip<T: DeserializeOwned>(body: &[u8], compressed_limit: usize, decompressed_limit: usize) -> Result<T>;
pub fn encode<T: Serialize + ?Sized>(value: &T) -> Vec<u8>;
pub fn write<T: Serialize + ?Sized>(status: StatusCode, value: &T) -> Response;
pub fn assert_denies_unknown_fields<T: DeserializeOwned>();
```

Failures: `413 request body too large`, `413 decompressed body too large`, `400 invalid gzip
body`, `400 invalid JSON body`, `400 request body must contain a single JSON object`.

## `env` (always)

```rust
pub struct Vars;  // process() | from_pairs(..)
impl Vars { get, string, required, int, bool, duration, list }
pub struct EnvError(pub String);   // displays "env: <reason>"
pub enum Environment { Development, Staging, Production }  // Environment::parse
pub struct Core { app_env, port: u16, log_level, database_url, cors_allowed_origins: Vec<String>,
    journal_url, journal_token, trusted_proxies: Vec<IpNet>, cdn_proxies: Vec<IpNet>, cdn_header }
impl Core { load(), load_without_database(), from_vars(&Vars, require_database: bool), is_production() }
pub const CORS_ORIGIN_KEYS: [&str; 7];
pub const DEFAULT_PORT: u16 = 8080;
pub fn cors_origins(vars: &Vars) -> Vec<String>;
pub fn trusted_proxies(vars: &Vars) -> Result<Vec<IpNet>, EnvError>;
pub fn cdn(vars: &Vars) -> (Vec<IpNet>, String);
```

## `proxies` (always)

```rust
pub const PRIVATE: [&str; 8];
pub const CLOUDFLARE: [&str; 22];
pub fn default_trusted() -> Vec<IpNet>;
pub fn cloudflare() -> Vec<IpNet>;
pub fn parse<I: IntoIterator<Item = S>, S: AsRef<str>>(values: I) -> Result<Vec<IpNet>, String>;
pub fn trusted_by(addr: IpAddr, trusted: &[IpNet]) -> bool;
pub fn parse_addr(value: &str) -> Option<IpAddr>;
pub fn split_host(value: &str) -> Option<&str>;
```

## `http` (`http`)

```rust
pub struct Config { pub security: SecurityConfig, pub cors: CorsConfig,
    pub trusted_proxies: Option<Vec<IpNet>>, pub cdn_proxies: Vec<IpNet>, pub cdn_header: String,
    pub api_prefix: Option<String> }
impl Config { pub fn from_core(core: &env::Core) -> Self }
pub fn layers(config: Config) -> Result<Layers, String>;  // impl tower::Layer, for Router::layer
pub struct Tls;                                            // request extension: TLS terminated here
pub struct RequestId(pub String);                          // extractor
pub struct RemoteAddr(pub String);                         // extractor: resolved client address
pub async fn not_found() -> Response;                      // Go's "404 page not found\n"
pub fn plain(status: StatusCode, body: impl Into<axum::body::Body>) -> Response;
pub struct SecurityConfig { csp_enforce, csp_sources: Sources, csp_report_uri, allow: Vec<String>,
    cross_origin_opener_policy, hsts_exclude_subdomains, trusted: Option<Vec<IpNet>> }
pub struct Security;  // new(&SecurityConfig, &[IpNet]) -> Result, apply, is_https
pub type Sources = BTreeMap<String, Vec<String>>;
pub fn build_csp(extra: &Sources, report_uri: &str) -> Result<String, String>;
pub struct SecurityOverrideConfig { csp_enforce: Option<bool>, csp_sources, allow, cross_origin_opener_policy }
pub struct SecurityOverride;  // new(config) -> Result, apply(&mut HeaderMap)
pub async fn security_override(State<Arc<SecurityOverride>>, Request, Next) -> Response;
pub struct CorsConfig { allowed_origins, allow_credentials, allowed_headers, allowed_methods,
    exposed_headers: Option<Vec<String>>, max_age_seconds: u32 }
pub struct RealIpConfig { trusted: Vec<IpNet>, cdn: Vec<IpNet>, header: String }
pub fn resolve_client(remote_addr: &str, headers: &HeaderMap, config: &RealIpConfig) -> String;
pub fn log_client_ip(headers: &HeaderMap, remote_addr: &str) -> String;
pub fn redact_query(raw: &str) -> String;
pub enum Kind { Api, Health, Static }
pub fn classify(path: &str, api_prefix: &str) -> Kind;
pub fn level_for(kind: Kind, status: u16) -> tracing::Level;
pub struct RateLimitConfig { limit: f64, burst: u32, key: Option<KeyFn>, status: Option<StatusCode> }
pub struct RateLimiter;  // new(config), allow(key, now) -> Result<(), u64>
pub async fn rate_limit(State<Arc<RateLimiter>>, Request, Next) -> Response;
```

Constants: `HSTS_VALUE`, `MAX_CSP_HEADER_BYTES`, `REQUEST_ID_HEADER`, `MAX_REQUEST_ID_LENGTH`,
`SENSITIVE_QUERY_KEYS`, `DEFAULT_ALLOWED_METHODS`, `DEFAULT_ALLOWED_HEADERS`,
`DEFAULT_EXPOSED_HEADERS`, `DEFAULT_RATE_LIMIT`, `RATE_LIMIT_EXCEEDED_BODY`.

## `health` and `healthcheck` (`health`)

```rust
pub const DEFAULT_TIMEOUT: Duration;                   // 2 s
pub struct Check;                                      // Check::new(async fn -> bool), Check::db(PgPool)
pub fn routes<S>(checks: Vec<Check>) -> Router<S>;     // GET /health, /ready, /api/health, /api/ready
pub fn handle(args: impl IntoIterator<Item = String>) -> bool;  // healthcheck::handle
pub fn probe(port: &str) -> Result<(), String>;                 // healthcheck::probe
```

| Route | Answer |
|---|---|
| `GET /health`, `GET /api/health` | `200 {"status":"ok"}` |
| `GET /ready`, `GET /api/ready` | `200 {"status":"ready"}` or `503 {"status":"not_ready"}` |
| any other verb | `405`, empty body |

## `spa` (`spa`)

```rust
pub const DEFAULT_DIR: &str = "./client";
pub const IMMUTABLE_PREFIXES: [&str; 3];
pub struct Config { pub dir: PathBuf, pub index: String }
pub fn dir_from_env() -> PathBuf;
pub fn available(dir: &Path) -> bool;
pub fn handler<S>(config: Config) -> MethodRouter<S>;  // Router::fallback_service
pub fn inline_script_hashes(dir: &Path) -> io::Result<Vec<String>>;
```

## `migrate` (`migrate`)

```rust
pub async fn run(pool: &PgPool, migrator: &Migrator) -> Result<()>;
pub async fn command(args: &[String], pool: &PgPool, migrator: &Migrator) -> Option<Result<()>>;
pub enum Subcommand { Up, Status, Version, Baseline(i64) }
pub fn parse(args: &[String]) -> Result<Subcommand>;
```

## `testdb` (`testdb`)

```rust
pub const ENV_VAR: &str = "TEST_DATABASE_URL";
pub const TIMEOUT: Duration;
pub fn url() -> Option<String>;
pub fn skip_reason(bootstrap: &str) -> String;
pub fn schema_name(prefix: &str) -> String;
pub async fn pool(prefix: &str, migrator: &Migrator) -> Option<PgPool>;
pub async fn truncate(pool: &PgPool, migrator: &Migrator, keep: &[&str]) -> sqlx::Result<()>;
```

## `apiref` (`apiref`)

```rust
pub const SCALAR_SCRIPT_URL: &str;   // @scalar/api-reference@1.64.0 on jsDelivr, as Go
pub const SCALAR_FONT_ORIGIN: &str;  // https://fonts.scalar.com
pub const BASE_PATH: &str = "/docs";
pub const SPEC_PATH: &str = "/docs/openapi.json";
pub const DEFAULT_VERSION: &str = "1.0.0";
pub struct Config { pub title: String, pub version: String, pub description: String,
    pub servers: Vec<String>, pub script_url: String }
pub fn document(config: &Config, doc: utoipa::openapi::OpenApi) -> OpenApi;
pub fn routes<S>(config: &Config, doc: OpenApi) -> Result<Router<S>, String>;
pub fn undocumented<S>(router: &Router<S>, doc: &OpenApi, config: &Config, ignore: &[&str]) -> Vec<String>;
pub fn mounted<S>(router: &Router<S>) -> BTreeMap<String, Vec<String>>;
```

| Route | Answer |
|---|---|
| `GET /docs` | The Scalar page, with `script-src <bundle URL>` and `font-src 'self' https://fonts.scalar.com` added to the app's CSP on that route only |
| `GET /docs/openapi.json` | The document, `Cache-Control: public, max-age=3600` |

The drift test every app keeps:

```rust
let missing = tronc::apiref::undocumented(&app, &doc, &config, &[]);
assert!(missing.is_empty(), "routes missing from the API document: {missing:?}");
```

## `mcp` (`mcp`)

A remote MCP server on rmcp 3.9's Streamable HTTP transport, stateless (spec 2026-07-28): no
`Mcp-Session-Id`, any replica answers any request, `GET` and `DELETE` answer 405. Legacy
clients (`initialize`, 2025-11-25 and older) are served statelessly too.

```rust
pub use rmcp;                                   // the SDK, for model types and schemars

pub trait Verifier: Send + Sync + 'static {     // the app's auth kit implements it
    fn verify<'a>(&'a self, token: &'a str, request: &'a http::request::Parts) -> VerifyFuture<'a>;
}
pub type VerifyFuture<'a> = Pin<Box<dyn Future<Output = Result<TokenInfo, VerifyError>> + Send + 'a>>;
pub enum VerifyError { Invalid /* 401 */, Unavailable /* 500 */ }
pub struct TokenInfo { pub user_id: String, pub scopes: Vec<String>,
    pub expires_at: Option<SystemTime>, pub extra: HashMap<String, String> }
pub const EXTRA_CLIENT_ID: &str = "client_id";  // porte mcpauth.ExtraClientID
pub const EXTRA_ACTOR: &str = "actor";          // porte mcpauth.ExtraActor

pub struct Options { name, title, version, instructions, resource, issuer, verifier: Arc<dyn Verifier>,
    scopes: Scopes, rate_limit: http::RateLimitConfig, origins: Vec<String>, tools: Vec<Tool>,
    list_ttl: Duration }                          // all pub
impl Options { pub fn new(name, resource, issuer, verifier) -> Self }
pub struct Scopes { pub read: String, pub write: String }   // empty: <name>.read, <name>.write
pub const DEFAULT_LIST_TTL: Duration;           // 10 minutes
pub struct Server;
impl Server {
    pub fn new(options: Options) -> Result<Self, String>;
    pub fn router<S: Clone + Send + Sync + 'static>(&self) -> Router<S>;
    pub fn metadata_url(&self) -> &str;
}
pub fn mount<S>(router: Router<S>, options: Options) -> Result<Router<S>, String>;

pub struct ToolSpec { pub name: String, pub title: String, pub description: String,
    pub annotations: Option<Annotations> }      // None is refused at boot
pub struct Annotations { pub read_only: bool, pub destructive: bool, pub idempotent: bool, pub open_world: bool }
pub fn tool<In, Out, F, Fut>(spec: ToolSpec, handler: F) -> Tool
    where In: DeserializeOwned + JsonSchema, Out: Serialize + JsonSchema,
          F: Fn(Call, In) -> Fut, Fut: Future<Output = Result<Answer<Out>, ToolError>> + Send;
pub enum Answer<Out> { Output(Out), InputRequired(rmcp::model::InputRequiredResult) }  // From<Out>
pub struct Call { pub caller: Caller, pub parts: Parts, pub protocol_version, pub capabilities,
    pub request_state: Option<String>, pub input_responses: InputResponses }
impl Call { pub fn can_elicit(&self) -> bool; pub fn retrying(&self) -> bool }
pub struct Caller { pub user_id, pub client_id, pub actor, pub scopes }  // has(scope), from_token(&TokenInfo)

pub struct ToolError(pub String);               // sent as an isError result; From<tronc::Error>
pub fn failure(error: tronc::Error, hint: &str) -> ToolError;
pub const UNAVAILABLE: &str;

pub struct Confirmations;                       // new(ConfirmConfig) -> Result<Self, String>
impl Confirmations {
    pub async fn request(&self, call: &Call, kind: &str, target: &str, question: &str)
        -> Result<(Option<InputRequiredResult>, Outcome)>;
    pub async fn resume(&self, call: &Call, kind: &str) -> Result<Outcome>;
    pub async fn confirm(&self, who: &Caller, id: &str) -> Result<Outcome>;   // confirm page
    pub async fn dismiss(&self, who: &Caller, id: &str) -> Result<()>;
    pub async fn open(&self, user_id: &str) -> Result<Vec<Action>>;
    pub async fn state(&self, action: &Action) -> &'static str;              // ready, changed, missing
    pub fn link(&self, id: &str) -> String;
}
pub struct ConfirmConfig { pub store: Arc<dyn Pending>, pub key: Vec<u8>, pub confirm_url: String,
    pub kinds: HashMap<String, Kind>, pub pending_ttl: Duration, pub ask_ttl: Duration }
pub struct Kind { pub tool, pub fingerprint, pub run }   // Kind::new(tool, async fingerprint, async run)
pub struct Outcome { pub status: &'static str, pub action: Option<Action>, pub confirm_url: String }
pub trait Pending { create, open, claim, finish, release, resolve }   // claim is atomic
pub struct Action { id, user_id, kind, target, digest, summary, client_id, created_at, expires_at }
pub struct PostgresPending { pub pool: PgPool }
pub const PENDING_SCHEMA: &str;                 // migrations/mcp/0001_mcp_actions.sql
pub fn gone() -> Error; pub fn changed() -> Error; pub fn is_changed(&Error) -> bool;
pub fn invalid_confirmation() -> Error;         // and the GONE, CHANGED, INVALID_CONFIRMATION texts

pub const RESULT_BUDGET: usize = 75_000;        // about 25k tokens
pub fn fit_count(total: usize, size: impl Fn(usize) -> usize) -> usize;
pub fn encoded_size<T: Serialize>(value: &T) -> usize;
pub fn clip(text: &str, limit: usize) -> (&str, bool);
pub fn encode_cursor<T: Serialize>(position: &T) -> String;
pub fn decode_cursor<T: DeserializeOwned>(cursor: &str) -> Result<Option<T>, ToolError>;
pub fn detailed(format: &str) -> Result<bool, ToolError>;   // response_format
pub fn audit(event: Event<'_>);                 // info, target "mcp", never arguments
pub struct ProtectedResourceMetadata; pub fn metadata_url(resource: &str) -> Option<String>;
```

| Route | Answer |
|---|---|
| `POST <resource path>` | Origin check (403), bearer token (401 or 403 with an RFC 6750 challenge), write-scope gate (403 `insufficient_scope` naming the write scope), per-person rate limit (429), then the transport |
| `GET\|DELETE <resource path>` | 405 once authenticated |
| `GET /.well-known/oauth-protected-resource<path>` and the bare well-known path | The RFC 9728 document, `Access-Control-Allow-Origin: *` |

Mount it on the root router, outside `/api` and outside CSRF middleware:

```rust
let verifier: Arc<dyn tronc::mcp::Verifier> = porte_verifier;      // tronc never imports porte
let mut options = tronc::mcp::Options::new("feuilles", resource, issuer, verifier);
options.tools = tools;          // tronc::mcp::tool(spec, handler), named feuilles_<verb>
let app = tronc::mcp::mount(app, options)?;
```

The app copies `migrations/mcp/0001_mcp_actions.sql` into its own migrations under its next
number, and builds `ConfirmConfig { store: Arc::new(PostgresPending::new(pool)), .. }`.

## `mcptest` (`mcptest`)

The conformance kit, a port of Go `mcp/mcptest`, for an app's own tests.

```rust
pub async fn run(config: &Config);              // panics on the first failure
pub struct Config { pub url: String, pub token: TokenFn, pub confirm: Vec<ConfirmCase> }
pub struct ConfirmCase { pub tool: String, pub setup: Arc<dyn Fn() -> Fut<Target> + Send + Sync> }
pub struct Target { pub args: JsonObject, pub runs: Arc<dyn Fn() -> Fut<usize> + Send + Sync> }
pub async fn connect(endpoint: &str, token: &str, options: ClientOptions) -> Client;  // rmcp client
pub struct ClientOptions { pub elicit: Option<Elicit>, pub version: Option<ProtocolVersion> }
pub async fn call(client: &Client, name: &str, args: JsonObject) -> CallToolResult;
pub async fn call_once(client: &Client, params: CallToolRequestParams) -> CallToolResponse;
pub fn decode<T: DeserializeOwned>(result: &CallToolResult) -> T;
pub fn text(result: &CallToolResult) -> String;
pub fn answering(action: ElicitationAction, asked: Arc<AtomicUsize>) -> Elicit;
pub async fn post(endpoint, token, body, headers) -> (StatusCode, HeaderMap);
pub struct MemoryPending;                       // in-memory Pending, outcome(id) for assertions
```

## `logger` and `journal` (always; Journal needs `journal`)

```rust
pub struct Config { pub level: String, pub journal: Option<journal::Config> }  // journal field with the feature
impl Config { pub fn from_core(core: &env::Core) -> Self }
pub struct Guard;                      // drop flushes Journal
pub fn init(config: Config) -> Guard;
pub fn parse_level(level: &str) -> tracing::Level;

pub const MAX_SERVER_BATCH: usize = 1000;
pub struct Config { url, token, app, flush_interval, max_batch, buffer_cap }  // journal::Config::new(url, token)
pub struct Entry { app, level, message, ts, meta }
pub struct Client;                     // new, log, dropped, layer, close
pub struct JournalLayer;               // tracing_subscriber::Layer
```
