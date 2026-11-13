use std::fmt::Write;

use sha2::{Digest as _, Sha256};

use super::pending::Action;
use crate::errors::{Error, Result};

/// Fingerprints the fields of a target for [`super::Kind`]'s `fingerprint`: the hex of the first
/// 16 bytes of SHA-256 over the fields joined by NUL, as Go `mcp.Digest`. A digest stored by a Go
/// app or by an earlier release stays valid.
pub fn digest<I, S>(fields: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut hash = Sha256::new();
    for (index, field) in fields.into_iter().enumerate() {
        if index > 0 {
            hash.update([0]);
        }
        hash.update(field.as_ref().as_bytes());
    }
    hash.finalize()[..16].iter().fold(String::with_capacity(32), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// Reads an action whose user id and target are both decimal ids, as Go `mcp.TargetIDs`, or
/// returns `refusal` when either is not.
pub fn target_ids(action: &Action, refusal: Error) -> Result<(i64, i64)> {
    match (action.user_id.parse(), action.target.parse()) {
        (Ok(user_id), Ok(target_id)) => Ok((user_id, target_id)),
        _ => Err(refusal),
    }
}

/// The key that signs an app's confirmation state, derived from the app's secret as Go
/// `mcp.StateKey`: SHA-256 of `<app>-mcp-state:` followed by `secret`. Every Go app derived its
/// key this way before the helper existed, so a Rust app on the same secret signs the same state.
pub fn state_key(app: &str, secret: &str) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(app.as_bytes());
    hash.update(b"-mcp-state:");
    hash.update(secret.as_bytes());
    hash.finalize().to_vec()
}
