use super::csp::token_name;

const OPENER_POLICIES: [&str; 4] = ["same-origin", "same-origin-allow-popups", "noopener-allow-popups", "unsafe-none"];

pub fn opener(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok("same-origin".to_owned());
    }
    if !OPENER_POLICIES.contains(&value) {
        return Err(format!(
            "tronc/middleware: Cross-Origin-Opener-Policy {value:?} is not one of [{}]",
            OPENER_POLICIES.join(" ")
        ));
    }
    Ok(value.to_owned())
}

pub fn permissions(allow: &[String]) -> Result<String, String> {
    if let Some(bad) = allow.iter().find(|feature| !token_name(feature)) {
        return Err(format!("tronc/middleware: Permissions-Policy feature {bad:?} is not a feature name"));
    }
    Ok(grant("camera=(), microphone=(), geolocation=()", allow))
}

pub fn grant(policy: &str, allow: &[String]) -> String {
    let mut names = Vec::new();
    let mut parts = Vec::new();
    for part in policy.split(',') {
        let (name, value) = part.trim().split_once('=').unwrap_or((part.trim(), ""));
        if name.is_empty() {
            continue;
        }
        let value = if allow.iter().any(|a| a == name) { "(self)" } else { value };
        names.push(name.to_owned());
        parts.push(format!("{name}={value}"));
    }
    for feature in allow {
        if !names.contains(feature) {
            names.push(feature.clone());
            parts.push(format!("{feature}=(self)"));
        }
    }
    parts.join(", ")
}
