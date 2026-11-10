use std::collections::BTreeMap;

/// Query parameters whose values are credentials, replaced with `[redacted]` before logging.
pub const SENSITIVE_QUERY_KEYS: [&str; 11] = [
    "access_token",
    "api_key",
    "apikey",
    "code",
    "id_token",
    "key",
    "password",
    "refresh_token",
    "secret",
    "signature",
    "token",
];

type Values = BTreeMap<Vec<u8>, Vec<Vec<u8>>>;

/// Replaces the value of every sensitive parameter, as Go `middleware.RedactQuery`: a query
/// with nothing to redact is kept verbatim, a redacted one is re-encoded with sorted keys, and
/// one that does not parse is reported as `[unparsable]`.
pub fn redact_query(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let Some(mut values) = parse_query(raw) else { return "[unparsable]".to_owned() };
    let mut redacted = false;
    for (key, list) in &mut values {
        let lower = String::from_utf8_lossy(key).to_lowercase();
        if SENSITIVE_QUERY_KEYS.contains(&lower.as_str()) {
            *list = vec![b"[redacted]".to_vec()];
            redacted = true;
        }
    }
    if !redacted {
        return raw.to_owned();
    }
    encode_query(&values)
}

fn parse_query(raw: &str) -> Option<Values> {
    let mut values = Values::new();
    let mut failed = false;
    for piece in raw.split('&') {
        if piece.contains(';') {
            failed = true;
            continue;
        }
        if piece.is_empty() {
            continue;
        }
        let (key, value) = piece.split_once('=').unwrap_or((piece, ""));
        match (unescape(key), unescape(value)) {
            (Some(key), Some(value)) => values.entry(key).or_default().push(value),
            _ => failed = true,
        }
    }
    (!failed).then_some(values)
}

fn unescape(raw: &str) -> Option<Vec<u8>> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3).filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))?;
                out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    Some(out)
}

fn escape(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    for &byte in raw {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn encode_query(values: &Values) -> String {
    let mut parts = Vec::new();
    for (key, list) in values {
        for value in list {
            parts.push(format!("{}={}", escape(key), escape(value)));
        }
    }
    parts.join("&")
}
