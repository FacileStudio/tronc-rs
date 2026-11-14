#![cfg(all(feature = "http", feature = "health", feature = "spa", feature = "mcp", feature = "bulle"))]

mod conformance {
    pub mod assert;
    pub mod bulle;
    pub mod client_ip;
    pub mod errors;
    pub mod health;
    pub mod json_decode;
    pub mod mcp_http;
    pub mod mcp_values;
    pub mod request_log;
    pub mod scripts;
    pub mod security_headers;
    pub mod spa;
    pub mod support;
}

use conformance::support::{Case, load_vectors};
use conformance::*;

async fn run_case(subject: &str, case: &Case) -> Result<(), String> {
    match subject {
        "errors" => errors::check(case).await,
        "json_decode" => json_decode::check(case).await,
        "health" => health::check(case).await,
        "request_log" => request_log::check(case),
        "security_headers" => security_headers::check(case).await,
        "client_ip" => client_ip::check(case).await,
        "spa" => spa::check(case).await,
        "inline_script_hashes" => scripts::check(case),
        "mcp_http" => mcp_http::http_case(case).await,
        "mcp_rate_limit" => mcp_http::rate_limit_case(case).await,
        "mcp_digest" => mcp_values::digest_case(case),
        "mcp_state_key" => mcp_values::state_key_case(case),
        "mcp_failure" => mcp_values::failure_case(case),
        "bulle_errors" => bulle::errors_case(case).await,
        "bulle_paths" => bulle::paths_case(case).await,
        other => Err(format!("no runner for subject {other:?}: add tests/conformance/{other}.rs and its arm here")),
    }
}

#[tokio::test]
async fn every_vector_passes() {
    let vectors = load_vectors();
    assert!(!vectors.is_empty(), "tests/vectors holds no vector file; run scripts/sync-vectors.sh");
    let mut failures = Vec::new();
    let mut ran = 0;
    for vector in &vectors {
        for case in &vector.cases {
            ran += 1;
            if let Err(reason) = run_case(&vector.subject, case).await {
                failures.push(format!("{}/{}: {reason}", vector.subject, case.name));
            }
        }
    }
    assert!(failures.is_empty(), "{} of {ran} vector cases failed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn the_vectors_come_from_a_go_release_tag() {
    let recorded =
        std::fs::read_to_string(conformance::support::vectors_dir().join("SOURCE")).expect("tests/vectors/SOURCE");
    let tag = recorded.trim();
    let release = tag
        .strip_prefix('v')
        .is_some_and(|v| v.split('.').count() == 3 && v.split('.').all(|n| n.parse::<u32>().is_ok()));
    assert!(release, "tests/vectors/SOURCE names {tag:?}; sync from a release tag so CI can re-sync it");
    for vector in load_vectors() {
        assert!(
            vector.source.starts_with("github.com/FacileStudio/tronc@"),
            "{} names {}",
            vector.subject,
            vector.source
        );
    }
}
