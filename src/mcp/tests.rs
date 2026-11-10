use std::collections::HashSet;
use std::time::Duration;

use super::gate::calls_any;
use super::signer::{INVALID_CONFIRMATION, Signer};

#[test]
fn the_signer_refuses_forgeries() {
    let signer = Signer { key: b"k".to_vec(), ttl: Duration::from_secs(60) };
    let state = signer.sign("7", "abc", 1_000);
    assert_eq!(signer.verify(&state, "7", 1_000).expect("own state"), "abc");
    let forged = state.replace(".abc.", ".abd.");
    let (payload, mac) = state.rsplit_once('.').expect("mac");
    let upper = format!("{payload}.{}", mac.to_uppercase());
    let other_key = Signer { key: b"j".to_vec(), ttl: Duration::ZERO };
    let refused = [
        signer.verify(&forged, "7", 1_000),
        signer.verify(&state, "8", 1_000),
        signer.verify(&state, "7", 1_001),
        other_key.verify(&state, "7", 1_000),
        signer.verify("abc", "7", 1_000),
        signer.verify(&state, "", 1_000),
        signer.verify(&upper, "7", 1_000),
    ];
    for (case, result) in refused.into_iter().enumerate() {
        let error = result.expect_err("a forgery must fail");
        assert_eq!(error.message, INVALID_CONFIRMATION, "case {case}");
    }
}

#[test]
fn the_write_gate_reads_batches() {
    let writes = HashSet::from(["app_write".to_owned()]);
    let batch = br#"[{"method":"tools/list"},{"method":"tools/call","params":{"name":"app_write"}}]"#;
    assert!(calls_any(batch, &writes), "the gate must catch a write call in a batch");
    assert!(!calls_any(br#"{"method":"tools/call","params":{"name":"app_read"}}"#, &writes));
    assert!(calls_any(br#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"app_write"}}"#, &writes));
    assert!(!calls_any(b"not json", &writes));
}
