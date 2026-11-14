use super::failure::ToolError;
use super::page::{decode_cursor, encode_cursor};

/// Reads the position of an offset cursor made by [`cursor`], as Go `mcp.Offset`: 0 when the
/// cursor is empty, [`INVALID_CURSOR`] when it is malformed.
pub fn offset(cursor: &str) -> Result<usize, ToolError> {
    Ok(decode_cursor::<usize>(cursor)?.unwrap_or(0))
}

/// The cursor of the page starting at `offset` in a list of `total` items, empty when `offset`
/// is past the end, as Go `mcp.Cursor`.
pub fn cursor(offset: usize, total: usize) -> String {
    if offset >= total { String::new() } else { encode_cursor(&offset) }
}

/// Cuts the page of at most `size` items, at least one, at the cursor's offset, and returns the
/// cursor of the next page, empty on the last one, as Go `mcp.Page`. Trim the page further with
/// [`fit_count`] and mint the cursor with [`cursor`] when its items may pass the budget.
pub fn page<'a, T>(items: &'a [T], at: &str, size: usize) -> Result<(&'a [T], String), ToolError> {
    let start = offset(at)?.min(items.len());
    let end = (start + size.max(1)).min(items.len());
    Ok((&items[start..end], cursor(end, items.len())))
}
