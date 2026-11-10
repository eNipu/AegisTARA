//! Find in page (UI-16): the filter text of the table pages, matched against
//! an element's visual ID, name and description. Plain text matches without
//! regard to case; a regular expression is compiled once, with a size cap.
//! The `regex` crate runs in time linear in the text, so no time cap is
//! needed beyond the size of the compiled program.

use regex::{Regex, RegexBuilder};

/// Longest filter text, in chars.
pub const MAX_FIND_CHARS: usize = 256;

/// Cap on the compiled program, in bytes (the `regex` default is 10 MiB).
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// A compiled filter.
#[derive(Clone, Debug)]
pub struct Find {
    text: String,
    regex: bool,
    matcher: Matcher,
}

#[derive(Clone, Debug)]
enum Matcher {
    /// Lowercased needle.
    Plain(String),
    Regex(Regex),
    /// The text cannot be used; nothing matches until it is fixed.
    Invalid(FindError),
}

/// Why a find text matches nothing (the window words it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindError {
    /// Longer than [`MAX_FIND_CHARS`].
    TooLong,
    /// Not a valid regular expression.
    Invalid,
    /// The compiled expression is over the size cap.
    TooLarge,
}

impl Find {
    /// Compiles `text`; an empty text gives `None` (no filter).
    #[must_use]
    pub fn new(text: &str, regex: bool) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let matcher = if text.chars().count() > MAX_FIND_CHARS {
            Matcher::Invalid(FindError::TooLong)
        } else if regex {
            match RegexBuilder::new(text)
                .case_insensitive(true)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
            {
                Ok(r) => Matcher::Regex(r),
                Err(regex::Error::CompiledTooBig(_)) => Matcher::Invalid(FindError::TooLarge),
                Err(_) => Matcher::Invalid(FindError::Invalid),
            }
        } else {
            Matcher::Plain(text.to_lowercase())
        };
        Some(Self {
            text: text.to_owned(),
            regex,
            matcher,
        })
    }

    /// The text as typed.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether the text is a regular expression.
    #[must_use]
    pub const fn is_regex(&self) -> bool {
        self.regex
    }

    /// Why the filter matches nothing, when the text is not usable.
    #[must_use]
    pub const fn error(&self) -> Option<FindError> {
        match &self.matcher {
            Matcher::Invalid(e) => Some(*e),
            _ => None,
        }
    }

    /// Whether any of `fields` matches. Time O(total length).
    #[must_use]
    pub fn hit(&self, fields: &[&str]) -> bool {
        match &self.matcher {
            Matcher::Plain(needle) => fields.iter().any(|f| f.to_lowercase().contains(needle)),
            Matcher::Regex(r) => fields.iter().any(|f| r.is_match(f)),
            Matcher::Invalid(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_regex_japanese_and_errors() {
        assert!(Find::new("", false).is_none());
        let f = Find::new("ecu", false).expect("find");
        assert!(f.hit(&["A01", "Brake ECU"]) && !f.hit(&["A02", "Gateway"]));
        let f = Find::new("ブレーキ", false).expect("find");
        assert!(f.hit(&["ブレーキECUの改ざん"]));
        let f = Find::new("^T0[12]$", true).expect("find");
        assert!(f.hit(&["T01", "x"]) && !f.hit(&["T03", "T011"]));
        let f = Find::new("(", true).expect("find");
        assert_eq!(f.error(), Some(FindError::Invalid));
        assert!(!f.hit(&["("]));
        // Plain text is literal: "(" is fine without the regex toggle.
        assert!(Find::new("(", false).expect("find").hit(&["f(x)"]));
        let f = Find::new("a{1000}{1000}", true).expect("find");
        assert_eq!(f.error(), Some(FindError::TooLarge));
        let f = Find::new(&"x".repeat(MAX_FIND_CHARS + 1), false).expect("find");
        assert_eq!(f.error(), Some(FindError::TooLong));
    }
}
