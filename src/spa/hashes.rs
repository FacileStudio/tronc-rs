use std::io;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use super::scan::scan_scripts;

/// Returns a CSP source, `'sha256-…'`, for every inline script in the built client's `.html`
/// files, sorted and without duplicates, as Go `spa.InlineScriptHashes`. Pass the result as the
/// `script-src` entry of the security configuration so an enforced `script-src 'self'` still
/// runs SvelteKit's bootstrap.
pub fn inline_script_hashes(dir: &Path) -> io::Result<Vec<String>> {
    let mut hashes = Vec::new();
    walk(dir, &mut hashes)?;
    hashes.sort();
    hashes.dedup();
    Ok(hashes)
}

fn walk(dir: &Path, hashes: &mut Vec<String>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(&path, hashes)?;
        } else if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("html")) {
            let page = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
            hashes.extend(hash_inline_scripts(&page));
        }
    }
    Ok(())
}

/// Hashes each script element without a `src` attribute, after the newline normalisation the
/// HTML parser applies.
pub fn hash_inline_scripts(page: &str) -> Vec<String> {
    let page = page.replace("\r\n", "\n").replace('\r', "\n");
    scan_scripts(&page)
        .into_iter()
        .filter(|script| !script.text.is_empty() && !script.attributes.iter().any(|name| name == "src"))
        .map(|script| format!("'sha256-{}'", STANDARD.encode(Sha256::digest(script.text.as_bytes()))))
        .collect()
}
