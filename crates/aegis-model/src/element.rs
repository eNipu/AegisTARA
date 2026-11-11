//! Generic element kinds: kinds the engine never computes on.
//!
//! A generic kind (misuse case now; parties, requirements and glossary later)
//! is stored in one shared element store instead of its own arena. The engine
//! never reads these elements when it evaluates risk. A new kind is one row in
//! [`KINDS`] plus one [`ElementKind`] variant.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::{EntityKind, ModelError};
use crate::id::{ElementId, ThreatId, parse_visual_num};
use crate::op::Op;
use crate::project::{Project, check_name, check_provenance};
use crate::provenance::Provenance;

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

/// An element of a generic kind: its kind, name, provenance and extra
/// attributes (links come in 205d).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    kind: ElementKind,
    name: Arc<str>,
    provenance: Provenance,
    /// Sorted by key, keys unique.
    attributes: Vec<(Arc<str>, AttrValue)>,
}

impl Element {
    /// The kind of this element. Time O(1).
    #[must_use]
    pub fn kind(&self) -> ElementKind {
        self.kind
    }

    /// The name of this element. Time O(1).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Who last changed this element, when and from what source. Time O(1).
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// The extra attributes, sorted by key. Time O(1).
    #[must_use]
    pub fn attributes(&self) -> &[(Arc<str>, AttrValue)] {
        &self.attributes
    }

    /// The value of one attribute. Time O(log a).
    #[must_use]
    pub fn attribute(&self, key: &str) -> Option<&AttrValue> {
        let i = self
            .attributes
            .binary_search_by(|(k, _)| k.as_ref().cmp(key))
            .ok()?;
        self.attributes.get(i).map(|(_, v)| v)
    }
}

/// The elements of every generic kind in one project.
#[derive(Clone, Debug, Default)]
pub(crate) struct ElementStore {
    kinds: BTreeMap<ElementKind, KindSlot>,
    /// Every live element.
    #[allow(clippy::zero_sized_map_values)] // one kind today; more rows add variants
    kind_of: BTreeMap<ElementId, ElementKind>,
}

#[derive(Clone, Debug, Default)]
struct KindSlot {
    /// Visual number and element, in ID order.
    live: BTreeMap<ElementId, (u32, Element)>,
    next_visual: u32,
    /// So an undone removal gets its number back.
    removed_visuals: BTreeMap<ElementId, u32>,
}

impl ElementStore {
    /// The element with this ID. Time O(log n).
    pub(crate) fn get(&self, id: ElementId) -> Option<&Element> {
        let kind = self.kind_of.get(&id)?;
        self.kinds.get(kind)?.live.get(&id).map(|(_, e)| e)
    }

    /// The element with this ID, mutably. Time O(log n).
    pub(crate) fn get_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        let kind = self.kind_of.get(&id)?;
        self.kinds.get_mut(kind)?.live.get_mut(&id).map(|(_, e)| e)
    }

    /// The visual number of this element. Time O(log n).
    pub(crate) fn visual_num(&self, id: ElementId) -> Option<u32> {
        let kind = self.kind_of.get(&id)?;
        self.kinds.get(kind)?.live.get(&id).map(|(n, _)| *n)
    }

    /// Live elements of `kind`. Time O(1).
    pub(crate) fn count(&self, kind: ElementKind) -> usize {
        self.kinds.get(&kind).map_or(0, |k| k.live.len())
    }

    /// Live elements of `kind` in ID order.
    pub(crate) fn iter(
        &self,
        kind: ElementKind,
    ) -> impl Iterator<Item = (ElementId, &Element)> + '_ {
        self.kinds
            .get(&kind)
            .into_iter()
            .flat_map(|k| k.live.iter().map(|(id, (_, e))| (*id, e)))
    }

    /// Inserts an element. Its number is the one this ID had when removed, else
    /// the next unused one; numbers are never reused. Time O(log n).
    pub(crate) fn insert(&mut self, id: ElementId, element: Element) {
        let kind = element.kind;
        let slot = self.kinds.entry(kind).or_default();
        let number = slot.removed_visuals.remove(&id).unwrap_or(slot.next_visual);
        slot.next_visual = slot.next_visual.max(number.saturating_add(1));
        slot.live.insert(id, (number, element));
        self.kind_of.insert(id, kind);
    }

    /// Removes an element, remembering its number. Time O(log n).
    pub(crate) fn remove(&mut self, id: ElementId) -> Option<Element> {
        let kind = self.kind_of.remove(&id)?;
        let slot = self.kinds.get_mut(&kind)?;
        let (number, element) = slot.live.remove(&id)?;
        slot.removed_visuals.insert(id, number);
        Some(element)
    }

    /// Whether both stores hold the same live elements per kind, ignoring
    /// visual numbers and removed numbers. Time O(n).
    pub(crate) fn same_live(&self, other: &Self) -> bool {
        ElementKind::ALL
            .iter()
            .all(|k| self.iter(*k).eq(other.iter(*k)))
    }
}

impl Project {
    /// Iterates the live elements of `kind` in ID order. Time O(1) to start, O(1) per item.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ElementId, ElementKind, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ElementId::new_v7();
    /// project.apply(Op::AddElement {
    ///     id,
    ///     kind: ElementKind::MisuseCase,
    ///     name: "Spoofed key fob".into(),
    ///     provenance: Provenance::human("alice", 1),
    /// })?;
    /// let names: Vec<_> = project.elements(ElementKind::MisuseCase).map(|(_, e)| e.name()).collect();
    /// assert_eq!(names, ["Spoofed key fob"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn elements(&self, kind: ElementKind) -> impl Iterator<Item = (ElementId, &Element)> + '_ {
        self.elements.iter(kind)
    }

    /// The element with this ID. Time O(log n).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ElementId, Methodology, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert!(project.element(ElementId::new_v7()).is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(id)
    }

    /// The number of live elements of `kind`. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ElementKind, Methodology, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.element_count(ElementKind::MisuseCase), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn element_count(&self, kind: ElementKind) -> usize {
        self.elements.count(kind)
    }

    /// The visual ID of an element (`MU00`, ...). Time O(log n).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ElementId, ElementKind, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ElementId::new_v7();
    /// project.apply(Op::AddElement {
    ///     id,
    ///     kind: ElementKind::MisuseCase,
    ///     name: "Spoofed key fob".into(),
    ///     provenance: Provenance::human("alice", 1),
    /// })?;
    /// assert_eq!(project.element_visual_id(id).as_deref(), Some("MU00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn element_visual_id(&self, id: ElementId) -> Option<String> {
        let num = self.elements.visual_num(id)?;
        Some(self.elements.get(id)?.kind.visual_id(num))
    }

    fn live_element(&self, id: ElementId) -> Result<&Element, ModelError> {
        self.elements.get(id).ok_or(ModelError::UnknownElement(id))
    }

    /// Checks the generic element ops; `None` for any other op.
    pub(crate) fn check_element_op(&self, op: &Op) -> Option<Result<(), ModelError>> {
        Some(match op {
            Op::AddElement {
                id,
                kind,
                name,
                provenance,
            } => self.check_add_element(*id, *kind, name, provenance),
            Op::RenameElement { id, name, .. } => self
                .live_element(*id)
                .and_then(|_| check_name(EntityKind::Element, name)),
            Op::RemoveElement { id } => self.check_remove_element(*id),
            Op::SetElementAttribute {
                id,
                key,
                value,
                provenance,
            } => self.check_set_element_attribute(*id, key, value.as_ref(), provenance),
            _ => return None,
        })
    }

    fn check_remove_element(&self, id: ElementId) -> Result<(), ModelError> {
        let count = self.live_element(id)?.attributes.len();
        if count > 0 {
            return Err(ModelError::ElementHasAttributes { element: id, count });
        }
        Ok(())
    }

    fn check_set_element_attribute(
        &self,
        id: ElementId,
        key: &str,
        value: Option<&AttrValue>,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        let e = self.live_element(id)?;
        if !is_key(key) {
            return Err(ModelError::InvalidAttributeKey { element: id });
        }
        if value.is_some_and(|v| !is_attribute_value(v)) {
            return Err(ModelError::InvalidAttributeValue { element: id });
        }
        if value.is_some()
            && e.attribute(key).is_none()
            && e.attributes.len() >= MAX_ELEMENT_ATTRIBUTES
        {
            return Err(ModelError::TooManyAttributes {
                element: id,
                max: MAX_ELEMENT_ATTRIBUTES,
            });
        }
        check_provenance(provenance)
    }

    fn check_add_element(
        &self,
        id: ElementId,
        kind: ElementKind,
        name: &str,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.elements.get(id).is_some() {
            return Err(ModelError::DuplicateElement(id));
        }
        check_name(EntityKind::Element, name)?;
        check_provenance(provenance)?;
        if self.elements.count(kind) >= kind.def().max_elements {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Element,
            });
        }
        Ok(())
    }

    /// Commits `AddElement` and returns its inverse.
    pub(crate) fn commit_add_element(
        &mut self,
        id: ElementId,
        kind: ElementKind,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Op {
        self.elements.insert(
            id,
            Element {
                kind,
                name,
                provenance,
                attributes: Vec::new(),
            },
        );
        Op::RemoveElement { id }
    }

    /// Commits `RenameElement` and returns its inverse.
    pub(crate) fn commit_rename_element(
        &mut self,
        id: ElementId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let e = self
            .elements
            .get_mut(id)
            .ok_or(ModelError::UnknownElement(id))?;
        let name = std::mem::replace(&mut e.name, name);
        let provenance = std::mem::replace(&mut e.provenance, provenance);
        Ok(Op::RenameElement {
            id,
            name,
            provenance,
        })
    }

    /// Commits `SetElementAttribute` and returns its inverse.
    pub(crate) fn commit_set_element_attribute(
        &mut self,
        id: ElementId,
        key: Arc<str>,
        value: Option<AttrValue>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let e = self
            .elements
            .get_mut(id)
            .ok_or(ModelError::UnknownElement(id))?;
        let found = e.attributes.binary_search_by(|(k, _)| k.cmp(&key));
        let old = match (found, value) {
            (Ok(i), Some(v)) => e
                .attributes
                .get_mut(i)
                .map(|slot| std::mem::replace(&mut slot.1, v)),
            (Ok(i), None) => Some(e.attributes.remove(i).1),
            (Err(i), Some(v)) => {
                e.attributes.insert(i, (Arc::clone(&key), v));
                None
            }
            (Err(_), None) => None,
        };
        let provenance = std::mem::replace(&mut e.provenance, provenance);
        Ok(Op::SetElementAttribute {
            id,
            key,
            value: old,
            provenance,
        })
    }

    /// Commits `RemoveElement` and returns its inverse.
    pub(crate) fn commit_remove_element(&mut self, id: ElementId) -> Result<Op, ModelError> {
        let e = self
            .elements
            .remove(id)
            .ok_or(ModelError::UnknownElement(id))?;
        Ok(Op::AddElement {
            id,
            kind: e.kind,
            name: e.name,
            provenance: e.provenance,
        })
    }
}

/// `[a-z][a-z0-9_-]{0,63}`, the pack key syntax.
pub(crate) fn is_key(s: &str) -> bool {
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    first_ok
        && s.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// 1..=`MAX_ATTRIBUTE_TEXT_CHARS` chars, not blank after trim, no control characters.
pub(crate) fn is_attribute_text(s: &str) -> bool {
    let n = s.chars().take(MAX_ATTRIBUTE_TEXT_CHARS + 1).count();
    (1..=MAX_ATTRIBUTE_TEXT_CHARS).contains(&n)
        && !s.trim().is_empty()
        && !s.chars().any(char::is_control)
}

/// Level and Choice: `is_key`; Text: `is_attribute_text`.
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
