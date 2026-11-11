//! Generic element kinds: kinds the engine never computes on.
//!
//! A generic kind (misuse case now; parties, requirements and glossary later)
//! is stored in one shared element store instead of its own arena. The engine
//! never reads these elements when it evaluates risk. A new kind is one row in
//! [`KINDS`] plus one [`ElementKind`] variant.

use std::sync::Arc;

use crate::id::{ThreatId, parse_visual_num};

/// Most live elements of one kind in a project.
pub const MAX_ELEMENTS_PER_KIND: usize = 10_000;
/// Most links on one element.
pub const MAX_ELEMENT_LINKS: usize = 256;
/// Most attributes on one element.
pub const MAX_ELEMENT_ATTRIBUTES: usize = 32;
/// Most characters of a [`AttrValue::Text`] value.
pub const MAX_ATTRIBUTE_TEXT_CHARS: usize = 256;

/// A kind of generic element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ElementKind {
    /// A misuse case (`MU00`, ...).
    MisuseCase,
}

/// The kind of thing an element may link to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum TargetKind {
    /// A threat scenario.
    Threat,
}

/// The target of one element link.
///
/// ```
/// use aegis_model::{LinkTarget, TargetKind, ThreatId};
///
/// let t = LinkTarget::Threat(ThreatId::new_v7());
/// assert_eq!(t.kind(), TargetKind::Threat);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum LinkTarget {
    /// A threat scenario.
    Threat(ThreatId),
}

impl LinkTarget {
    /// The kind of this target. Time O(1).
    #[must_use]
    pub const fn kind(self) -> TargetKind {
        match self {
            Self::Threat(_) => TargetKind::Threat,
        }
    }
}

/// A pack-declared extra attribute value (D34). Level: a key of an ordered scale;
/// Choice: a key of an unordered list; Text: free text. Job 176 checks them against the pack.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttrValue {
    /// A key of an ordered scale.
    Level(Arc<str>),
    /// A key of an unordered list.
    Choice(Arc<str>),
    /// Free text.
    Text(Arc<str>),
}

/// One row of the kernel kind table.
#[derive(Debug, PartialEq, Eq)]
pub struct KindDef {
    /// The kind this row describes.
    pub kind: ElementKind,
    /// File and op log key, pack key syntax: `"misuse-case"`.
    pub key: &'static str,
    /// Visual ID prefix: `"MU"`.
    pub prefix: &'static str,
    /// Workspace file `systems/main/<stem>.json`: `"misuse"`.
    pub file_stem: &'static str,
    /// English singular for messages: `"misuse case"`.
    pub label: &'static str,
    /// What an element of this kind may link to.
    pub targets: &'static [TargetKind],
    /// Most live elements of this kind (at most [`MAX_ELEMENTS_PER_KIND`]).
    pub max_elements: usize,
    /// Most links per element (at most [`MAX_ELEMENT_LINKS`]).
    pub max_links: usize,
}

const MISUSE_CASE: KindDef = KindDef {
    kind: ElementKind::MisuseCase,
    key: "misuse-case",
    prefix: "MU",
    file_stem: "misuse",
    label: "misuse case",
    targets: &[TargetKind::Threat],
    max_elements: MAX_ELEMENTS_PER_KIND,
    max_links: MAX_ELEMENT_LINKS,
};

/// The kernel kind table, in [`ElementKind`] order.
pub const KINDS: &[KindDef] = &[MISUSE_CASE];

impl ElementKind {
    /// Every kind, in table order.
    pub const ALL: &'static [ElementKind] = &[ElementKind::MisuseCase];

    /// The table row of this kind. Time O(1).
    ///
    /// ```
    /// use aegis_model::ElementKind;
    ///
    /// assert_eq!(ElementKind::MisuseCase.def().label, "misuse case");
    /// ```
    #[must_use]
    pub const fn def(self) -> &'static KindDef {
        match self {
            Self::MisuseCase => &MISUSE_CASE,
        }
    }

    /// The file and op log key. Time O(1).
    ///
    /// ```
    /// use aegis_model::ElementKind;
    ///
    /// assert_eq!(ElementKind::MisuseCase.key(), "misuse-case");
    /// ```
    #[must_use]
    pub const fn key(self) -> &'static str {
        self.def().key
    }

    /// The visual ID prefix. Time O(1).
    ///
    /// ```
    /// use aegis_model::ElementKind;
    ///
    /// assert_eq!(ElementKind::MisuseCase.prefix(), "MU");
    /// ```
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        self.def().prefix
    }

    /// Finds the kind with this key. Time O(kinds).
    ///
    /// ```
    /// use aegis_model::ElementKind;
    ///
    /// assert_eq!(ElementKind::from_key("misuse-case"), Some(ElementKind::MisuseCase));
    /// assert_eq!(ElementKind::from_key("nope"), None);
    /// ```
    #[must_use]
    pub fn from_key(key: &str) -> Option<ElementKind> {
        KINDS.iter().find(|d| d.key == key).map(|d| d.kind)
    }

    /// Whether elements of this kind may link to `target`. Time O(1).
    ///
    /// ```
    /// use aegis_model::{ElementKind, TargetKind};
    ///
    /// assert!(ElementKind::MisuseCase.allows(TargetKind::Threat));
    /// ```
    #[must_use]
    pub fn allows(self, target: TargetKind) -> bool {
        self.def().targets.contains(&target)
    }

    /// Formats a visual ID, zero-padded to at least 2 digits. Time O(1).
    ///
    /// ```
    /// use aegis_model::ElementKind;
    ///
    /// assert_eq!(ElementKind::MisuseCase.visual_id(7), "MU07");
    /// assert_eq!(ElementKind::MisuseCase.visual_id(100), "MU100");
    /// ```
    #[must_use]
    pub fn visual_id(self, num: u32) -> String {
        format!("{}{num:02}", self.prefix())
    }

    /// Parses a canonical visual ID of this kind (rules of
    /// `VisualPrefix::parse_num`). Time O(1).
    ///
    /// ```
    /// use aegis_model::ElementKind;
    ///
    /// assert_eq!(ElementKind::MisuseCase.parse_visual("MU07"), Some(7));
    /// assert_eq!(ElementKind::MisuseCase.parse_visual("MU7"), None);
    /// ```
    #[must_use]
    pub fn parse_visual(self, text: &str) -> Option<u32> {
        parse_visual_num(self.prefix(), text)
    }
}

/// `[a-z][a-z0-9_-]{0,63}`, the pack key syntax.
#[allow(dead_code)] // used by the element store in slice 205c
pub(crate) fn is_key(s: &str) -> bool {
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    first_ok
        && s.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// 1..=`MAX_ATTRIBUTE_TEXT_CHARS` chars, not blank after trim, no control characters.
#[allow(dead_code)] // used by the element store in slice 205c
pub(crate) fn is_attribute_text(s: &str) -> bool {
    let n = s.chars().take(MAX_ATTRIBUTE_TEXT_CHARS + 1).count();
    (1..=MAX_ATTRIBUTE_TEXT_CHARS).contains(&n)
        && !s.trim().is_empty()
        && !s.chars().any(char::is_control)
}

/// Level and Choice: `is_key`; Text: `is_attribute_text`.
#[allow(dead_code)] // used by the element store in slice 205c
pub(crate) fn is_attribute_value(v: &AttrValue) -> bool {
    match v {
        AttrValue::Level(k) | AttrValue::Choice(k) => is_key(k),
        AttrValue::Text(t) => is_attribute_text(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_syntax() {
        assert!(is_key("a"));
        assert!(is_key("misuse-case_2"));
        assert!(is_key(&"a".repeat(64)));
        assert!(!is_key(&"a".repeat(65)));
        assert!(!is_key(""));
        assert!(!is_key("1a"));
        assert!(!is_key("A"));
        assert!(!is_key("a b"));
        assert!(!is_key("-a"));
    }

    #[test]
    fn attribute_text_rules() {
        assert!(is_attribute_text("hello"));
        assert!(is_attribute_text(&"é".repeat(MAX_ATTRIBUTE_TEXT_CHARS)));
        assert!(!is_attribute_text(
            &"é".repeat(MAX_ATTRIBUTE_TEXT_CHARS + 1)
        ));
        assert!(!is_attribute_text(""));
        assert!(!is_attribute_text("   "));
        assert!(!is_attribute_text("a\nb"));
    }

    #[test]
    fn attribute_value_dispatch() {
        assert!(is_attribute_value(&AttrValue::Level("high".into())));
        assert!(!is_attribute_value(&AttrValue::Choice("High".into())));
        assert!(is_attribute_value(&AttrValue::Text("Any text".into())));
        assert!(!is_attribute_value(&AttrValue::Text("".into())));
    }
}
