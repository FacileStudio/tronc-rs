use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;

use super::Tls;
use super::client_ip::{RealIpConfig, RemoteAddr, peer_string, resolve};
use super::override_policy::Pending;
use super::security::Security;

pub struct Compiled {
    pub security: Security,
    pub real_ip: RealIpConfig,
}

pub async fn security_headers(State(compiled): State<Arc<Compiled>>, request: Request, next: Next) -> Response {
    let tls = request.extensions().get::<Tls>().is_some();
    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
    let https = compiled.security.is_https(tls, peer, request.headers());
    let mut response = next.run(request).await;
    compiled.security.apply(response.headers_mut(), https);
    if let Some(Pending(route)) = response.extensions_mut().remove::<Pending>() {
        route.apply(response.headers_mut());
    }
    response
}

pub async fn real_ip(State(compiled): State<Arc<Compiled>>, mut request: Request, next: Next) -> Response {
    let peer = peer_string(request.extensions());
    let resolved = resolve(&peer, request.headers(), &compiled.real_ip);
    request.extensions_mut().insert(RemoteAddr(resolved));
    next.run(request).await
}
