use std::net::SocketAddr;
use std::process::ExitCode;

use axum::Router;
use tronc::{env::Core, health, healthcheck, http, logger, spa};

fn main() -> ExitCode {
    if healthcheck::handle(std::env::args()) {
        return ExitCode::SUCCESS;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build();
    runtime.map(|runtime| runtime.block_on(run())).unwrap_or(ExitCode::FAILURE)
}

async fn run() -> ExitCode {
    let core = match Core::load_without_database() {
        Ok(core) => core,
        Err(error) => return fail(&error),
    };
    let _guard = logger::init(logger::Config::from_core(&core));
    let layers = match http::layers(http::Config::from_core(&core)) {
        Ok(layers) => layers,
        Err(error) => return fail(&error),
    };
    let dir = spa::dir_from_env();
    let router: Router = if spa::available(&dir) {
        health::routes(Vec::new()).fallback_service(spa::handler(spa::Config { dir, ..Default::default() }))
    } else {
        health::routes(Vec::new()).fallback(http::not_found)
    };
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", core.port)).await {
        Ok(listener) => listener,
        Err(error) => return fail(&error),
    };
    let app = router.layer(layers).into_make_service_with_connect_info::<SocketAddr>();
    let shutdown = async { _ = tokio::signal::ctrl_c().await };
    match axum::serve(listener, app).with_graceful_shutdown(shutdown).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(&error),
    }
}

fn fail(error: &dyn std::fmt::Display) -> ExitCode {
    tracing::error!(error = %error, "minimal: startup failed");
    eprintln!("minimal: {error}");
    ExitCode::FAILURE
}
