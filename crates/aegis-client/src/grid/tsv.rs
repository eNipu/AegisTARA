//! Size-capped strict RFC-4180 TSV formatter and parser for grid clipboard
//! copy and paste.

use crate::grid::{GridError, GridSelection};
use crate::table::TableRow;

/// Maximum clipboard TSV input size in bytes (1 MiB).
pub const MAX_PASTE_BYTES: usize = 1024 * 1024;
/// Maximum number of rows accepted by [`parse_tsv`].
pub const MAX_PASTE_ROWS: usize = 1_000;
/// Maximum number of columns per row accepted by [`parse_tsv`].
pub const MAX_PASTE_COLS: usize = 64;

/// Formats the rectangular cell range selected by `sel` in `rows` (indexed
/// through `visible_col_indices`) as RFC-4180 tab-separated text joined by
/// `\n`.
///
/// Returns an empty string if `rows` or `visible_col_indices` is empty or the
/// selection starts out of bounds. Cells containing `\t`, `\n`, `\r`, or `"`
/// are double-quoted with embedded `"` doubled to `""`.
///
/// Time: O(selected_rows * selected_cols * cell_len).
#[must_use]
pub fn format_tsv(rows: &[TableRow], visible_col_indices: &[usize], sel: GridSelection) -> String {
    if rows.is_empty() || visible_col_indices.is_empty() {
        return String::new();
    }
    let row_start = *sel.row_range().start();
    let col_start = *sel.col_range().start();
    if row_start >= rows.len() || col_start >= visible_col_indices.len() {
        return String::new();
    }
    let row_end = (*sel.row_range().end()).min(rows.len() - 1);
    let col_end = (*sel.col_range().end()).min(visible_col_indices.len() - 1);

    let mut out = String::new();
    for (r_offset, row) in rows[row_start..=row_end].iter().enumerate() {
        if r_offset > 0 {
            out.push('\n');
        }
        for (c_offset, &col_idx) in visible_col_indices[col_start..=col_end].iter().enumerate() {
            if c_offset > 0 {
                out.push('\t');
            }
            let text = row.cells.get(col_idx).map_or("", |c| c.text.as_str());
            push_tsv_field(&mut out, text);
        }
    }
    out
}

fn push_tsv_field(out: &mut String, text: &str) {
    if text.contains(['\t', '\n', '\r', '"']) {
        out.push('"');
        for ch in text.chars() {
            if ch == '"' {
                out.push_str("\"\"");
            } else {
                out.push(ch);
            }
        }
        out.push('"');
    } else {
        out.push_str(text);
    }
}

/// Parses untrusted clipboard TSV text strictly according to RFC-4180 with
/// `\t` field delimiters and `\r\n` or `\n` row endings.
///
/// Strips at most one trailing `\r\n` or `\n` before checking for empty input,
/// and enforces [`MAX_PASTE_BYTES`], [`MAX_PASTE_ROWS`], and [`MAX_PASTE_COLS`]
/// during parsing so hostile inputs cannot allocate unbounded vectors.
///
/// Time: O(input.len()).
///
/// # Errors
///
/// - [`GridError::PasteTooLarge`] if `input.len() > MAX_PASTE_BYTES`.
/// - [`GridError::EmptyPaste`] if `input` is empty after stripping at most one
///   trailing `\r\n` or `\n`.
/// - [`GridError::TooManyRows`] if more than [`MAX_PASTE_ROWS`] rows are present.
/// - [`GridError::TooManyCols`] if any row has more than [`MAX_PASTE_COLS`] columns.
/// - [`GridError::MalformedQuote`] with the 1-based line number on unclosed
///   quotes, quotes inside unquoted fields, or trailing characters after a
///   closing quote.
pub fn parse_tsv(input: &str) -> Result<Vec<Vec<String>>, GridError> {
    if input.len() > MAX_PASTE_BYTES {
        return Err(GridError::PasteTooLarge);
    }
    let stripped = input
        .strip_suffix("\r\n")
        .or_else(|| input.strip_suffix('\n'))
        .unwrap_or(input);
    if stripped.is_empty() {
        return Err(GridError::EmptyPaste);
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut line: usize = 1;
    let mut chars = stripped.chars().peekable();

    loop {
        if row.is_empty() && rows.len() >= MAX_PASTE_ROWS {
            return Err(GridError::TooManyRows);
        }
        if row.len() >= MAX_PASTE_COLS {
            return Err(GridError::TooManyCols);
        }

        let (field, delim) = if chars.peek() == Some(&'"') {
            parse_quoted_field(&mut chars, &mut line)?
        } else {
            parse_unquoted_field(&mut chars, line)?
        };
        row.push(field);

        match delim {
            FieldEnd::Tab => {}
            FieldEnd::Newline => {
                rows.push(row);
                row = Vec::new();
                line += 1;
            }
            FieldEnd::Eof => {
                rows.push(row);
                return Ok(rows);
            }
        }
    }
}

enum FieldEnd {
    Tab,
    Newline,
    Eof,
}

fn parse_quoted_field(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    line: &mut usize,
) -> Result<(String, FieldEnd), GridError> {
    let _open = chars.next();
    let open_line = *line;
    let mut field = String::new();
    loop {
        match chars.next() {
            None => return Err(GridError::MalformedQuote { line: open_line }),
            Some('"') => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    let delim = match chars.next() {
                        None => FieldEnd::Eof,
                        Some('\t') => FieldEnd::Tab,
                        Some('\n') => FieldEnd::Newline,
                        Some('\r') if chars.peek() == Some(&'\n') => {
                            chars.next();
                            FieldEnd::Newline
                        }
                        Some(_) => return Err(GridError::MalformedQuote { line: *line }),
                    };
                    return Ok((field, delim));
                }
            }
            Some('\r') => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                    field.push_str("\r\n");
                    *line += 1;
                } else {
                    field.push('\r');
                }
            }
            Some('\n') => {
                field.push('\n');
                *line += 1;
            }
            Some(ch) => field.push(ch),
        }
    }
}

fn parse_unquoted_field(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    line: usize,
) -> Result<(String, FieldEnd), GridError> {
    let mut field = String::new();
    loop {
        match chars.next() {
            None => return Ok((field, FieldEnd::Eof)),
            Some('"') => return Err(GridError::MalformedQuote { line }),
            Some('\t') => return Ok((field, FieldEnd::Tab)),
            Some('\n') => return Ok((field, FieldEnd::Newline)),
            Some('\r') => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                    return Ok((field, FieldEnd::Newline));
                }
                field.push('\r');
            }
            Some(ch) => field.push(ch),
        }
    }
}
