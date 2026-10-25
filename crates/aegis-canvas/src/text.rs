//! Deterministic label wrapping.
//!
//! The canvas has no font engine. It estimates text width as
//! `chars x size x AVERAGE_ADVANCE`, wraps greedily on spaces, breaks words
//! that are longer than a line, and ends with an ellipsis when the text
//! does not fit. The estimate is the same on every machine, so the screen,
//! the SVG, and the PDF break lines in the same places (P4).

/// Average glyph advance as a fraction of the font size, for a typical
/// proportional UI font. Slightly generous, so text rarely overflows.
pub const AVERAGE_ADVANCE: f32 = 0.55;

/// How many characters of `size` pixels fit in `width` pixels.
///
/// ```
/// use aegis_canvas::text::chars_that_fit;
/// assert_eq!(chars_that_fit(184.0, 13.0), 25);
/// ```
#[must_use]
pub fn chars_that_fit(width: f32, size: f32) -> usize {
    if width <= 0.0 || size <= 0.0 {
        return 0;
    }
    // Truncation is the intent: whole characters only.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (width / (size * AVERAGE_ADVANCE)).floor() as usize;
    n
}

/// Wraps `text` into at most `max_lines` lines of at most `max_chars`
/// characters. If text remains, the last line ends with `…`.
///
/// Words move to the next line when they do not fit; a word longer than a
/// whole line is broken. Runs of whitespace collapse to one space.
///
/// Time O(len(text)); space O(output).
///
/// ```
/// use aegis_canvas::text::wrap;
/// assert_eq!(wrap("Relay the key fob signal", 12, 2), vec!["Relay the", "key fob…"]);
/// assert_eq!(wrap("short", 12, 2), vec!["short"]);
/// ```
#[must_use]
pub fn wrap(text: &str, max_chars: usize, max_lines: usize) -> Vec<String> {
    if max_chars == 0 || max_lines == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut cur: Vec<char> = Vec::new();
    let mut truncated = false;
    let flush = |cur: &mut Vec<char>, lines: &mut Vec<String>| lines.push(cur.drain(..).collect());

    'words: for word in text.split_whitespace() {
        let chars: Vec<char> = word.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if cur.is_empty() && lines.len() >= max_lines {
                truncated = true;
                break 'words;
            }
            let remaining = chars.len() - i;
            let sep = usize::from(!cur.is_empty());
            if cur.len() + sep + remaining <= max_chars {
                if sep == 1 {
                    cur.push(' ');
                }
                cur.extend_from_slice(&chars[i..]);
                i = chars.len();
            } else if !cur.is_empty() && remaining <= max_chars {
                // The word fits on a line of its own: start a new line.
                flush(&mut cur, &mut lines);
            } else {
                // The word is longer than a line: fill what is left of this
                // line and carry the rest over.
                let used = cur.len() + sep;
                if used >= max_chars {
                    flush(&mut cur, &mut lines);
                    continue;
                }
                let room = max_chars - used;
                if sep == 1 {
                    cur.push(' ');
                }
                cur.extend_from_slice(&chars[i..i + room]);
                i += room;
                flush(&mut cur, &mut lines);
            }
        }
    }
    if !cur.is_empty() {
        flush(&mut cur, &mut lines);
    }
    if truncated && let Some(last) = lines.last_mut() {
        let mut chars: Vec<char> = last.chars().collect();
        if chars.len() >= max_chars {
            chars.truncate(max_chars - 1);
        }
        chars.push('…');
        *last = chars.into_iter().collect();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_stays_on_one_line() {
        assert_eq!(wrap("Open door", 20, 2), vec!["Open door"]);
        assert_eq!(wrap("", 20, 2), Vec::<String>::new());
        assert_eq!(wrap("   ", 20, 2), Vec::<String>::new());
    }

    #[test]
    fn wraps_on_spaces_and_collapses_whitespace() {
        assert_eq!(wrap("a  b\tc\nd", 3, 5), vec!["a b", "c d"]);
        assert_eq!(wrap("alpha beta gamma", 10, 3), vec!["alpha beta", "gamma"]);
    }

    #[test]
    fn long_words_are_broken_and_the_end_gets_an_ellipsis() {
        assert_eq!(wrap("abcdefghij", 4, 5), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("abcdefghij", 4, 2), vec!["abcd", "efg…"]);
        assert_eq!(wrap("one two three four", 7, 2), vec!["one two", "three…"]);
    }

    #[test]
    fn every_line_respects_the_limits() {
        let text = "Exploit the diagnostic interface over CAN to reflash the gateway firmware";
        for max_chars in 1..40 {
            for max_lines in 1..5 {
                let lines = wrap(text, max_chars, max_lines);
                assert!(lines.len() <= max_lines);
                assert!(
                    lines.iter().all(|l| l.chars().count() <= max_chars),
                    "{max_chars}/{max_lines}: {lines:?}"
                );
            }
        }
    }

    #[test]
    fn multibyte_text_is_counted_in_characters() {
        assert_eq!(wrap("ÄÖÜß ÄÖÜß", 4, 2), vec!["ÄÖÜß", "ÄÖÜß"]);
        assert_eq!(wrap("日本語のテキスト", 4, 2), vec!["日本語の", "テキスト"]);
        assert_eq!(
            wrap("日本語のテキストです", 4, 2),
            vec!["日本語の", "テキス…"]
        );
    }

    #[test]
    fn degenerate_limits_give_nothing() {
        assert!(wrap("x", 0, 2).is_empty());
        assert!(wrap("x", 5, 0).is_empty());
        assert_eq!(chars_that_fit(0.0, 13.0), 0);
        assert_eq!(chars_that_fit(100.0, 0.0), 0);
    }
}
