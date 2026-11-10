#![cfg(all(feature = "http", feature = "health", feature = "spa"))]

mod conformance {
    pub mod assert;
    pub mod client_ip;
    pub mod errors;
    pub mod health;
    pub mod json_decode;
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
        other => Err(format!("no runner for subject {other:?}")),
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
fn every_vector_names_the_recorded_source() {
    let recorded =
        std::fs::read_to_string(conformance::support::vectors_dir().join("SOURCE")).expect("tests/vectors/SOURCE");
    let recorded = recorded.lines().next().unwrap_or("").trim().to_owned();
    for vector in load_vectors() {
        assert_eq!(vector.source, recorded, "{} was generated from another tag", vector.subject);
    }
}
