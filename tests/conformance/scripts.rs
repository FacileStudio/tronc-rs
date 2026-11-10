use serde_json::json;
use tronc::spa::inline_script_hashes;

use super::assert::expect_eq;
use super::support::{Case, string_map, text};

pub fn check(case: &Case) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut files = string_map(case.input.get("files"));
    files.insert("index.html".into(), text(&case.input, "html").to_owned());
    for (name, body) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap_or(dir.path())).map_err(|e| e.to_string())?;
        std::fs::write(path, body).map_err(|e| e.to_string())?;
    }
    let hashes = inline_script_hashes(dir.path()).map_err(|e| e.to_string())?;
    expect_eq("hashes", json!({ "hashes": hashes }), case.want.clone())
}
