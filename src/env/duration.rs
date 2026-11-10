use std::time::Duration;

const UNITS: [(&str, f64); 8] =
    [("ns", 1.0), ("us", 1e3), ("µs", 1e3), ("μs", 1e3), ("ms", 1e6), ("s", 1e9), ("m", 60e9), ("h", 3600e9)];

/// Parses a Go duration string such as `300ms`, `1.5h` or `2h45m`. Negative values are refused,
/// because a `std::time::Duration` cannot hold them.
pub fn parse(raw: &str) -> Option<Duration> {
    let mut rest = raw.strip_prefix('+').unwrap_or(raw);
    if rest == "0" {
        return Some(Duration::ZERO);
    }
    if rest.is_empty() {
        return None;
    }
    let mut nanos = 0.0_f64;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len());
        let number: f64 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let (unit, scale) =
            UNITS.iter().filter(|(unit, _)| rest.starts_with(unit)).max_by_key(|(unit, _)| unit.len())?;
        rest = &rest[unit.len()..];
        nanos += number * scale;
    }
    Some(Duration::from_nanos(nanos.round() as u64))
}
