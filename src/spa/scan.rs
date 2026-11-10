pub struct Script<'a> {
    pub attributes: Vec<String>,
    pub text: &'a str,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r')
}

fn is_boundary(c: u8) -> bool {
    is_space(c) || c == b'/' || c == b'>'
}

/// Finds every script element, reading attributes as the HTML tokenizer does, so a quoted
/// value holding `>` or `src=` cannot end the tag early or pass for a `src` attribute.
pub fn scan_scripts(page: &str) -> Vec<Script<'_>> {
    let bytes = page.as_bytes();
    let lower = bytes.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(start) = find(&lower, b"<script", offset) {
        let after = start + b"<script".len();
        if after < bytes.len() && !is_boundary(bytes[after]) {
            offset = after;
            continue;
        }
        let Some((attributes, text_start)) = attributes(bytes, after) else { break };
        let Some(end) = closing(&lower, text_start) else { break };
        found.push(Script { attributes, text: &page[text_start..end] });
        offset = end;
    }
    found
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack.get(from..)?.windows(needle.len()).position(|window| window == needle).map(|i| i + from)
}

fn attributes(bytes: &[u8], mut i: usize) -> Option<(Vec<String>, usize)> {
    let mut names = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'>' {
            return Some((names, i + 1));
        }
        if is_space(c) || c == b'/' {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < bytes.len() && !is_boundary(bytes[end]) && bytes[end] != b'=' {
            end += 1;
        }
        names.push(String::from_utf8_lossy(&bytes[i..end]).to_ascii_lowercase());
        i = skip_value(bytes, end);
    }
    None
}

fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_space(bytes[i]) {
        i += 1;
    }
    i
}

fn skip_value(bytes: &[u8], i: usize) -> usize {
    let i = skip_spaces(bytes, i);
    if i >= bytes.len() || bytes[i] != b'=' {
        return i;
    }
    let mut i = skip_spaces(bytes, i + 1);
    if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
        let quote = bytes[i];
        return bytes[i + 1..].iter().position(|&c| c == quote).map_or(bytes.len(), |end| i + 1 + end + 1);
    }
    while i < bytes.len() && !is_space(bytes[i]) && bytes[i] != b'>' {
        i += 1;
    }
    i
}

fn closing(lower: &[u8], from: usize) -> Option<usize> {
    let mut offset = from;
    loop {
        let end = find(lower, b"</script", offset)?;
        let after = end + b"</script".len();
        if after >= lower.len() || is_boundary(lower[after]) {
            return Some(end);
        }
        offset = after;
    }
}
