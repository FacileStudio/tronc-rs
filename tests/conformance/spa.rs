use axum::Router;
use tronc::spa::{Config, handler};

use super::assert::{expect_answer, expect_eq};
use super::support::{Case, call, request, string_map, text};

pub async fn check(case: &Case) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let files = string_map(case.input.get("files"));
    for (name, body) in &files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap_or(dir.path())).map_err(|e| e.to_string())?;
        std::fs::write(path, body).map_err(|e| e.to_string())?;
    }
    let router = Router::new().fallback_service(handler(Config { dir: dir.path().to_path_buf(), ..Config::default() }));
    let answer = call(router, request(text(&case.input, "method"), text(&case.input, "path"), "")).await;
    expect_answer(&answer, &case.want)?;
    match case.want.get("served").and_then(|v| v.as_str()) {
        Some(served) => {
            expect_eq("served body", String::from_utf8_lossy(&answer.body).into_owned(), files[served].clone())
        }
        None => Ok(()),
    }
}
