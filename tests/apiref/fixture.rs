use axum::Router;
use tronc::apiref::{self, Config};
use tronc::http::{Config as ChainConfig, layers};
use utoipa::openapi::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/sheets", responses((status = 200, description = "Every sheet")))]
async fn list_sheets() -> &'static str {
    "[]"
}

#[utoipa::path(get, path = "/sheets/{id}", params(("id" = String, Path)), responses((status = 200, description = "One sheet")))]
async fn get_sheet() -> &'static str {
    "{}"
}

pub fn config() -> Config {
    Config { title: "Feuilles API".into(), servers: vec!["/api".into()], ..Config::default() }
}

pub fn app(extra: Router) -> (Router, OpenApi) {
    let (api, doc) = OpenApiRouter::new().routes(routes!(list_sheets)).routes(routes!(get_sheet)).split_for_parts();
    let docs: Router = apiref::routes(&config(), doc.clone()).expect("reference");
    let router = Router::new().nest("/api", api.merge(extra)).merge(docs).merge(tronc::health::routes(Vec::new()));
    (router.layer(layers(ChainConfig::default()).expect("chain")), doc)
}
