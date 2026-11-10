use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::middleware::from_fn_with_state;
use axum::routing::any;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use tower::ServiceBuilder;

use super::auth::{Gate, ProtectedResourceMetadata, WELL_KNOWN, require_token, serve_metadata};
use super::gate::{MAX_REQUEST_BODY_BYTES, WriteGate, origin_guard, write_gate};
use super::handler::{Handler, Toolset};
use super::options::{Options, normalize};
use super::tool::{Tool, check};
use crate::http::{RateLimiter, rate_limit};

/// A validated MCP server, ready to mount.
pub struct Server {
    opts: Options,
    path: String,
    metadata_url: String,
    allowed_origins: Vec<String>,
    writes: HashSet<String>,
    handler: Handler,
}

/// Builds the server and merges its routes into `router`; see [`Server::new`] and
/// [`Server::router`].
pub fn mount<S: Clone + Send + Sync + 'static>(router: Router<S>, options: Options) -> Result<Router<S>, String> {
    Ok(router.merge(Server::new(options)?.router()))
}

fn register(opts: &Options) -> Result<(BTreeMap<String, Tool>, HashSet<String>), String> {
    let prefix = format!("{}_", opts.name);
    let (mut seen, mut writes, mut tools) = (HashSet::new(), HashSet::new(), BTreeMap::new());
    for tool in &opts.tools {
        check(&tool.spec, &prefix, &mut seen)?;
        if !tool.spec.annotations.is_some_and(|a| a.read_only) {
            if opts.scopes.write.is_empty() {
                return Err(format!("mcp: tool {:?} writes, so the server needs a write scope", tool.spec.name));
            }
            writes.insert(tool.spec.name.clone());
        }
        tools.insert(tool.spec.name.clone(), tool.clone());
    }
    Ok((tools, writes))
}

fn server_info(opts: &Options) -> ServerConfig {
    let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
    let mut implementation = Implementation::new(opts.name.clone(), opts.version.clone());
    implementation.title = Some(opts.title.clone()).filter(|t| !t.is_empty());
    info.server_info = implementation;
    info.instructions = Some(opts.instructions.clone()).filter(|i| !i.is_empty());
    info
}

impl Server {
    /// Validates `opts` and registers the tools, so an app can fail at boot and mount later.
    pub fn new(mut opts: Options) -> Result<Self, String> {
        normalize(&mut opts)?;
        let resource: Option<http::Uri> = opts.resource.parse().ok();
        let Some((scheme, authority, path)) = resource.as_ref().and_then(|uri| {
            Some((uri.scheme_str()?, uri.authority()?.as_str(), uri.path())).filter(|(_, host, _)| !host.is_empty())
        }) else {
            return Err(format!("mcp: resource {:?} is not an absolute URI", opts.resource));
        };
        let origin = format!("{scheme}://{authority}");
        let path = if path.is_empty() { "/".to_owned() } else { path.to_owned() };
        let metadata_url = format!("{origin}{WELL_KNOWN}{}", path.trim_end_matches('/'));
        let mut allowed_origins = vec![origin];
        allowed_origins
            .extend(opts.origins.iter().filter(|o| !allowed_origins.contains(o)).cloned().collect::<Vec<_>>());
        let (tools, writes) = register(&opts)?;
        let toolset = Toolset {
            listed: tools.values().map(Tool::definition).collect(),
            tools,
            info: server_info(&opts),
            write_scope: opts.scopes.write.clone(),
            list_ttl: opts.list_ttl,
        };
        let handler = Handler(Arc::new(toolset));
        Ok(Self { opts, path, metadata_url, allowed_origins, writes, handler })
    }

    /// The RFC 9728 metadata URL of the resource.
    pub fn metadata_url(&self) -> &str {
        &self.metadata_url
    }

    /// The endpoint at the resource's path and its RFC 9728 metadata at the well-known path for
    /// that resource, plus the bare well-known path for clients that look only there. Merge it
    /// into the root router, outside any `/api` prefix and outside CSRF middleware.
    pub fn router<S: Clone + Send + Sync + 'static>(&self) -> Router<S> {
        let document =
            Arc::new(ProtectedResourceMetadata::new(&self.opts.resource, &self.opts.issuer, self.opts.scopes.list()));
        let metadata = any(serve_metadata).with_state(document);
        let scoped = format!("{WELL_KNOWN}{}", self.path.trim_end_matches('/'));
        let router = Router::new().route_service(&self.path, self.endpoint()).route(&scoped, metadata.clone());
        if scoped == WELL_KNOWN { router } else { router.route(WELL_KNOWN, metadata) }
    }

    fn endpoint(
        &self,
    ) -> tower::util::BoxCloneSyncService<Request, axum::response::Response, std::convert::Infallible> {
        let handler = self.handler.clone();
        let config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None)
            .with_max_request_body_bytes(MAX_REQUEST_BODY_BYTES)
            .disable_allowed_hosts();
        let transport =
            StreamableHttpService::new(move || Ok(handler.clone()), Arc::new(NeverSessionManager::default()), config);
        let limiter = Arc::new(RateLimiter::new(self.opts.rate_limit.clone()));
        let gate = Arc::new(Gate {
            verifier: self.opts.verifier.clone(),
            metadata_url: self.metadata_url.clone(),
            read_scope: self.opts.scopes.read.clone(),
        });
        let writes = (!self.writes.is_empty())
            .then(|| Arc::new(WriteGate::new(self.writes.clone(), &self.opts.scopes.write, &self.metadata_url)));
        let service = ServiceBuilder::new()
            .layer(from_fn_with_state(Arc::new(self.allowed_origins.clone()), origin_guard))
            .layer(from_fn_with_state(gate, require_token))
            .option_layer(writes.map(|w| from_fn_with_state(w, write_gate)))
            .layer(from_fn_with_state(limiter, rate_limit))
            .service(transport);
        tower::util::BoxCloneSyncService::new(service)
    }
}
