use axum::Router;
use axum::routing::get;
use http::{HeaderName, HeaderValue};
use tronc::env::{Core, Vars};
use tronc::http::{Config, RemoteAddr, layers};

use super::assert::expect_eq;
use super::support::{Case, call, request, string_map, text};

pub async fn check(case: &Case) -> Result<(), String> {
    let vars = Vars::from_pairs([("TRUSTED_PROXIES", text(&case.input, "trusted_proxies"))]);
    let core = match (Core::from_vars(&vars, false), case.want.get("error").and_then(|v| v.as_str())) {
        (Err(error), Some(want)) => return expect_eq("error", error.to_string().as_str(), want),
        (Err(error), None) => return Err(format!("refused: {error}")),
        (Ok(_), Some(want)) => return Err(format!("accepted a value Go refuses with {want:?}")),
        (Ok(core), None) => core,
    };
    let chain = layers(Config::from_core(&core))?;
    let router = Router::new().route("/", get(|RemoteAddr(addr): RemoteAddr| async move { addr })).layer(chain);
    let mut request = request("GET", "/", text(&case.input, "remote_addr"));
    for (name, value) in string_map(case.input.get("headers")) {
        let name = HeaderName::try_from(name).map_err(|e| e.to_string())?;
        request.headers_mut().insert(name, HeaderValue::try_from(value).map_err(|e| e.to_string())?);
    }
    let answer = call(router, request).await;
    expect_eq("ip", String::from_utf8_lossy(&answer.body).as_ref(), text(&case.want, "ip"))
}
