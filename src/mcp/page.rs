use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::failure::ToolError;

/// The cap on the encoded size of one tool result: about 25,000 tokens, counting three bytes a
/// token for JSON.
pub const RESULT_BUDGET: usize = 75_000;

/// The concise `response_format`, the default.
pub const FORMAT_CONCISE: &str = "concise";

/// The detailed `response_format`.
pub const FORMAT_DETAILED: &str = "detailed";

/// What a model sees when it sends a cursor this server did not mint.
pub const INVALID_CURSOR: &str = "the cursor is not valid. Omit it to start again from the first page";

/// Reports whether `format` asks for the detailed form. Empty means concise; any other value is
/// an error the model can fix.
pub fn detailed(format: &str) -> Result<bool, ToolError> {
    match format {
        "" | FORMAT_CONCISE => Ok(false),
        FORMAT_DETAILED => Ok(true),
        _ => Err(ToolError::new(r#"response_format must be "concise" or "detailed""#)),
    }
}

/// Makes an opaque cursor out of a position, any value that serializes to JSON.
pub fn encode_cursor<T: Serialize>(position: &T) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(position).unwrap_or_default())
}

/// Reads a cursor made by [`encode_cursor`]. An empty cursor is `None`; a malformed one is
/// [`INVALID_CURSOR`].
pub fn decode_cursor<T: DeserializeOwned>(cursor: &str) -> Result<Option<T>, ToolError> {
    if cursor.is_empty() {
        return Ok(None);
    }
    let raw = URL_SAFE_NO_PAD.decode(cursor).map_err(|_| ToolError::new(INVALID_CURSOR))?;
    serde_json::from_slice(&raw).map(Some).map_err(|_| ToolError::new(INVALID_CURSOR))
}

/// The largest `n` in `0..=total` for which `size(n)`, the encoded size of a result holding the
/// first `n` items, stays within [`RESULT_BUDGET`]. `size` must grow with `n`; a binary search
/// finds `n` in `log(total)` encodings.
pub fn fit_count(total: usize, size: impl Fn(usize) -> usize) -> usize {
    let (mut low, mut high) = (0, total + 1);
    while low < high {
        let mid = low + (high - low) / 2;
        if size(mid) > RESULT_BUDGET {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    low.saturating_sub(1)
}

/// The JSON size of `value`, for [`fit_count`].
pub fn encoded_size<T: Serialize + ?Sized>(value: &T) -> usize {
    serde_json::to_vec(value).map_or(0, |raw| raw.len())
}

/// Cuts `text` to at most `limit` bytes on a character boundary, trims the cut, and reports
/// whether anything was cut.
pub fn clip(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut cut = limit;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (text[..cut].trim(), true)
}
