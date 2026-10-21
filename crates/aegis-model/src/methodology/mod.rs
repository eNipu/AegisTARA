//! Methodology definitions: the vocabularies a project is checked against.
//!
//! The core model names no impact category, rating scale, security
//! property, threat taxonomy, or catalog. A [`MethodologyDef`] declares them
//! as plain data (the shape a pack file will have), and [`Methodology::new`]
//! validates it once. The validated [`Methodology`] hands out small typed
//! indices ([`ImpactCategory`], [`Level`], [`Property`], [`ThreatCategory`],
//! [`CatalogScheme`]) that ops and entities carry. Text keys are resolved to
//! indices once, at the boundary.

mod catalog;
mod iso21434;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

pub use catalog::{CatalogRef, CatalogSyntax, SegmentDef};

use crate::asset::PropertySet;
use crate::error::{MethodologyError, ModelError};

/// Most impact categories a methodology may declare.
pub const MAX_IMPACT_CATEGORIES: usize = 32;
/// Fewest levels in one impact category's rating scale.
pub const MIN_LEVELS: usize = 2;
/// Most levels in one impact category's rating scale.
pub const MAX_LEVELS: usize = 16;
/// Most security properties a methodology may declare (a [`PropertySet`] is
/// a `u64`).
pub const MAX_PROPERTIES: usize = 64;
/// Most threat categories a methodology may declare.
pub const MAX_THREAT_CATEGORIES: usize = 64;
/// Most catalog schemes a methodology may declare.
pub const MAX_CATALOG_SCHEMES: usize = 16;
/// Longest label, in chars.
pub const MAX_LABEL_CHARS: usize = 256;
/// Longest version string, in chars.
pub const MAX_VERSION_CHARS: usize = 64;
/// Longest key, in bytes (keys are ASCII).
pub const MAX_KEY_LEN: usize = 64;

// ---------------------------------------------------------------------------
// Definition: plain data a pack will provide.
// ---------------------------------------------------------------------------

/// A methodology as plain data, before validation.
///
/// All fields are public so tests and a later pack loader can build it.
/// [`Methodology::new`] checks it. Declaration order is meaningful: it fixes
/// the indices and the iteration order, and a scale's levels are listed
/// lowest first.
///
/// ```
/// use aegis_model::{
///     ImpactCategoryDef, LevelDef, Methodology, MethodologyDef, PropertyDef, ThreatCategoryDef,
/// };
///
/// # fn main() -> Result<(), aegis_model::MethodologyError> {
/// let def = MethodologyDef {
///     key: "tiny".into(),
///     version: "1".into(),
///     label: "Tiny".into(),
///     impact_categories: vec![ImpactCategoryDef {
///         key: "harm".into(),
///         label: "Harm".into(),
///         levels: vec![
///             LevelDef { key: "low".into(), label: "Low".into() },
///             LevelDef { key: "high".into(), label: "High".into() },
///         ],
///     }],
///     properties: vec![PropertyDef { key: "integrity".into(), label: "Integrity".into() }],
///     threat_categories: vec![ThreatCategoryDef {
///         key: "corruption".into(),
///         label: "Corruption".into(),
///         violates: vec!["integrity".into()],
///     }],
///     catalog_schemes: vec![],
/// };
/// let m = Methodology::new(def)?;
/// assert_eq!(m.key(), "tiny");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodologyDef {
    /// Stable key, for example `"iso21434-r155"`.
    pub key: Arc<str>,
    /// Version of this definition, 1 to 64 chars.
    pub version: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
    /// Impact categories, 1 to 32.
    pub impact_categories: Vec<ImpactCategoryDef>,
    /// Security properties, 1 to 64.
    pub properties: Vec<PropertyDef>,
    /// Threat categories, 1 to 64.
    pub threat_categories: Vec<ThreatCategoryDef>,
    /// Catalog schemes, 0 to 16.
    pub catalog_schemes: Vec<CatalogSchemeDef>,
}

/// One impact category and its ordered rating scale.
///
/// ```
/// use aegis_model::{ImpactCategoryDef, LevelDef};
///
/// let c = ImpactCategoryDef {
///     key: "harm".into(),
///     label: "Harm".into(),
///     levels: vec![
///         LevelDef { key: "low".into(), label: "Low".into() },
///         LevelDef { key: "high".into(), label: "High".into() },
///     ],
/// };
/// assert_eq!(c.levels.len(), 2);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImpactCategoryDef {
    /// Stable key, unique among impact categories.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
    /// The rating scale, lowest first, 2 to 16 levels.
    pub levels: Vec<LevelDef>,
}

/// One level of a rating scale.
///
/// ```
/// use aegis_model::LevelDef;
///
/// let l = LevelDef { key: "low".into(), label: "Low".into() };
/// assert_eq!(&*l.key, "low");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelDef {
    /// Stable key, unique within its category.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
}

/// One security property.
///
/// ```
/// use aegis_model::PropertyDef;
///
/// let p = PropertyDef { key: "integrity".into(), label: "Integrity".into() };
/// assert_eq!(&*p.label, "Integrity");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDef {
    /// Stable key, unique among properties.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
}

/// One threat category and the properties it violates.
///
/// ```
/// use aegis_model::ThreatCategoryDef;
///
/// let t = ThreatCategoryDef {
///     key: "corruption".into(),
///     label: "Corruption".into(),
///     violates: vec!["integrity".into()],
/// };
/// assert_eq!(t.violates.len(), 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatCategoryDef {
    /// Stable key, unique among threat categories.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
    /// Keys of the violated properties: non-empty, declared, no repeats.
    pub violates: Vec<Arc<str>>,
}

/// One catalog scheme and its ID syntax.
///
/// ```
/// use aegis_model::{CatalogSchemeDef, CatalogSyntax, SegmentDef};
///
/// let s = CatalogSchemeDef {
///     key: "capec".into(),
///     label: "CAPEC".into(),
///     syntax: CatalogSyntax {
///         prefix: "CAPEC-".into(),
///         segments: vec![SegmentDef { min: 1, max: 9999, digits: None }],
///         required: 1,
///     },
/// };
/// assert_eq!(s.syntax.segments.len(), 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogSchemeDef {
    /// Stable key, unique among catalog schemes.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
    /// The ID syntax.
    pub syntax: CatalogSyntax,
}

// ---------------------------------------------------------------------------
// Vocabulary indices.
// ---------------------------------------------------------------------------

/// Defines a 1-byte vocabulary index with no public constructor.
macro_rules! vocab_index {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u8);

        impl $name {
            /// Returns the position in its vocabulary, in declaration order.
            /// Time O(1).
            ///
            /// ```
            /// use aegis_model::Methodology;
            ///
            /// # fn main() -> Result<(), aegis_model::MethodologyError> {
            /// let m = Methodology::iso21434_r155()?;
            /// let positions: Vec<usize> = m.properties().map(|p| p.index()).collect();
            /// assert_eq!(positions, [0, 1, 2, 3, 4, 5]);
            /// # Ok(())
            /// # }
            /// ```
            #[must_use]
            pub fn index(self) -> usize {
                usize::from(self.0)
            }
        }
    };
}

vocab_index! {
    /// Index of an impact category in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let first = m.impact_categories().next();
    /// assert_eq!(first.and_then(|c| m.impact_category_key(c)), Some("safety"));
    /// # Ok(())
    /// # }
    /// ```
    ImpactCategory
}

vocab_index! {
    /// A position in one impact category's rating scale. 1 byte.
    ///
    /// `Ord` is the scale order (lowest = 0). Levels of different
    /// categories are not comparable in meaning, even though the type
    /// allows it.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let cat = m.impact_category("safety").ok_or(aegis_model::MethodologyError::NoImpactCategory)?;
    /// assert!(m.level(cat, "negligible") < m.level(cat, "severe"));
    /// # Ok(())
    /// # }
    /// ```
    Level
}

vocab_index! {
    /// Index of a security property in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.property("integrity").map(|p| p.index()), Some(1));
    /// # Ok(())
    /// # }
    /// ```
    Property
}

vocab_index! {
    /// Index of a threat category in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.threat_categories().count(), 6);
    /// # Ok(())
    /// # }
    /// ```
    ThreatCategory
}

vocab_index! {
    /// Index of a catalog scheme in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.catalog_scheme("r155-annex5").map(|s| s.index()), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    CatalogScheme
}

// Raw constructors, only where the crate needs them. A [`Methodology`] hands
// out every index; these serve entity views that rebuild an index from a
// stored position, and tests that need an out-of-range index.

impl ImpactCategory {
    /// Wraps a raw position (used by `DamageScenario::impacts`).
    pub(crate) fn new(i: u8) -> Self {
        ImpactCategory(i)
    }
}

impl Property {
    /// Wraps a raw position (used by `PropertySet::iter`).
    pub(crate) fn new(i: u8) -> Self {
        Property(i)
    }
}

#[cfg(test)]
impl Level {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        Level(i)
    }
}

#[cfg(test)]
impl CatalogScheme {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        CatalogScheme(i)
    }
}

// ---------------------------------------------------------------------------
// Validated methodology.
// ---------------------------------------------------------------------------

/// Key and label of one vocabulary entry.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    key: Arc<str>,
    label: Arc<str>,
}

/// A validated impact category.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CategoryData {
    entry: Entry,
    levels: Vec<Entry>,
    level_by_key: BTreeMap<Arc<str>, u8>,
}

/// A validated threat category.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ThreatCategoryData {
    entry: Entry,
    violates: PropertySet,
}

/// One validated vocabulary while it is built: the entries in declaration
/// order and the key-to-position map.
struct Vocab<T> {
    items: Vec<T>,
    by_key: BTreeMap<Arc<str>, u8>,
}

/// A validated catalog scheme.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SchemeData {
    entry: Entry,
    syntax: CatalogSyntax,
}

/// A validated methodology: the vocabularies a project is checked against.
///
/// Built only by [`Methodology::new`] (or a built-in such as
/// [`Methodology::iso21434_r155`], which goes through `new`). It hands out
/// typed vocabulary indices, resolves keys to indices in O(log m) (m = the
/// vocabulary size), and parses and formats catalog IDs.
///
/// ```
/// use aegis_model::Methodology;
///
/// # fn main() -> Result<(), aegis_model::MethodologyError> {
/// let m = Methodology::iso21434_r155()?;
/// assert_eq!(m.impact_categories().count(), 4);
/// assert_eq!(m.properties().count(), 6);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Methodology {
    key: Arc<str>,
    version: Arc<str>,
    label: Arc<str>,
    impact_categories: Vec<CategoryData>,
    impact_by_key: BTreeMap<Arc<str>, u8>,
    properties: Vec<Entry>,
    property_by_key: BTreeMap<Arc<str>, u8>,
    threat_categories: Vec<ThreatCategoryData>,
    threat_by_key: BTreeMap<Arc<str>, u8>,
    schemes: Vec<SchemeData>,
    scheme_by_key: BTreeMap<Arc<str>, u8>,
    /// Every declared property.
    all_properties: PropertySet,
}

/// Checks `[a-z][a-z0-9_-]{0,63}`. Time O(key length).
fn check_key(key: &Arc<str>) -> Result<(), MethodologyError> {
    let bytes = key.as_bytes();
    let first_ok = bytes.first().is_some_and(u8::is_ascii_lowercase);
    let rest_ok = bytes
        .iter()
        .skip(1)
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-');
    if first_ok && rest_ok && bytes.len() <= MAX_KEY_LEN {
        Ok(())
    } else {
        Err(MethodologyError::InvalidKey(Arc::clone(key)))
    }
}

/// Checks a label of the item named `key`. Time O(label length).
fn check_label(key: &Arc<str>, label: &str) -> Result<(), MethodologyError> {
    if label.trim().is_empty() {
        return Err(MethodologyError::BlankLabel {
            key: Arc::clone(key),
        });
    }
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(MethodologyError::LabelTooLong {
            key: Arc::clone(key),
            max: MAX_LABEL_CHARS,
        });
    }
    Ok(())
}

/// Checks a vocabulary size against `min..=max`.
fn check_count(
    count: usize,
    min: usize,
    max: usize,
    err: impl FnOnce() -> MethodologyError,
) -> Result<(), MethodologyError> {
    if (min..=max).contains(&count) {
        Ok(())
    } else {
        Err(err())
    }
}

/// Validates the key and label of one entry and records it in `by_key` at
/// position `pos`. `dup` builds the error for a repeated key. Time O(log m +
/// text length).
fn add_entry(
    key: &Arc<str>,
    label: &Arc<str>,
    pos: usize,
    by_key: &mut BTreeMap<Arc<str>, u8>,
    dup: impl Fn(Arc<str>) -> MethodologyError,
) -> Result<Entry, MethodologyError> {
    check_key(key)?;
    check_label(key, label)?;
    // Callers check the vocabulary size first, so `pos` always fits.
    let pos = u8::try_from(pos).map_err(|_| dup(Arc::clone(key)))?;
    if by_key.insert(Arc::clone(key), pos).is_some() {
        return Err(dup(Arc::clone(key)));
    }
    Ok(Entry {
        key: Arc::clone(key),
        label: Arc::clone(label),
    })
}

impl Methodology {
    /// Validates a definition and builds the methodology.
    ///
    /// Time O(m log m + text), where m is the total vocabulary size; space
    /// O(m).
    ///
    /// # Errors
    ///
    /// A [`MethodologyError`] naming the first rule the definition breaks,
    /// checked in declaration order: the methodology key, version, and
    /// label; then the impact categories (count, each key, label, scale
    /// size, and levels); the properties; the threat categories and their
    /// `violates`; and the catalog schemes and their syntax.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyDef, MethodologyError};
    ///
    /// let def = MethodologyDef {
    ///     key: "Not-A-Key".into(),
    ///     version: "1".into(),
    ///     label: "Bad".into(),
    ///     impact_categories: vec![],
    ///     properties: vec![],
    ///     threat_categories: vec![],
    ///     catalog_schemes: vec![],
    /// };
    /// assert_eq!(Methodology::new(def), Err(MethodologyError::InvalidKey("Not-A-Key".into())));
    /// ```
    pub fn new(def: MethodologyDef) -> Result<Self, MethodologyError> {
        check_key(&def.key)?;
        let version = def.version.trim();
        if version.is_empty() || def.version.chars().count() > MAX_VERSION_CHARS {
            return Err(MethodologyError::InvalidVersion);
        }
        check_label(&def.key, &def.label)?;

        let impacts = Self::build_impacts(&def.impact_categories)?;
        let properties = Self::build_properties(&def.properties)?;
        let all_properties = (0..properties.items.len())
            .filter_map(|i| u8::try_from(i).ok().map(Property))
            .collect();
        let threats = Self::build_threats(&def.threat_categories, &properties.by_key)?;
        let schemes = Self::build_schemes(&def.catalog_schemes)?;

        Ok(Methodology {
            key: def.key,
            version: def.version,
            label: def.label,
            impact_categories: impacts.items,
            impact_by_key: impacts.by_key,
            properties: properties.items,
            property_by_key: properties.by_key,
            threat_categories: threats.items,
            threat_by_key: threats.by_key,
            schemes: schemes.items,
            scheme_by_key: schemes.by_key,
            all_properties,
        })
    }

    fn build_impacts(defs: &[ImpactCategoryDef]) -> Result<Vocab<CategoryData>, MethodologyError> {
        check_count(defs.len(), 1, MAX_IMPACT_CATEGORIES, || {
            if defs.is_empty() {
                MethodologyError::NoImpactCategory
            } else {
                MethodologyError::TooManyImpactCategories {
                    max: MAX_IMPACT_CATEGORIES,
                }
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, c) in defs.iter().enumerate() {
            let entry = add_entry(&c.key, &c.label, pos, &mut by_key, |k| {
                MethodologyError::DuplicateImpactCategory(k)
            })?;
            check_count(c.levels.len(), MIN_LEVELS, MAX_LEVELS, || {
                MethodologyError::LevelCount {
                    category: Arc::clone(&c.key),
                    min: MIN_LEVELS,
                    max: MAX_LEVELS,
                }
            })?;
            let mut level_by_key = BTreeMap::new();
            let mut levels = Vec::with_capacity(c.levels.len());
            for (lpos, l) in c.levels.iter().enumerate() {
                levels.push(add_entry(&l.key, &l.label, lpos, &mut level_by_key, |k| {
                    MethodologyError::DuplicateLevel {
                        category: Arc::clone(&c.key),
                        level: k,
                    }
                })?);
            }
            out.push(CategoryData {
                entry,
                levels,
                level_by_key,
            });
        }
        Ok(Vocab { items: out, by_key })
    }

    fn build_properties(defs: &[PropertyDef]) -> Result<Vocab<Entry>, MethodologyError> {
        check_count(defs.len(), 1, MAX_PROPERTIES, || {
            if defs.is_empty() {
                MethodologyError::NoProperty
            } else {
                MethodologyError::TooManyProperties {
                    max: MAX_PROPERTIES,
                }
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, p) in defs.iter().enumerate() {
            out.push(add_entry(&p.key, &p.label, pos, &mut by_key, |k| {
                MethodologyError::DuplicateProperty(k)
            })?);
        }
        Ok(Vocab { items: out, by_key })
    }

    fn build_threats(
        defs: &[ThreatCategoryDef],
        property_by_key: &BTreeMap<Arc<str>, u8>,
    ) -> Result<Vocab<ThreatCategoryData>, MethodologyError> {
        check_count(defs.len(), 1, MAX_THREAT_CATEGORIES, || {
            if defs.is_empty() {
                MethodologyError::NoThreatCategory
            } else {
                MethodologyError::TooManyThreatCategories {
                    max: MAX_THREAT_CATEGORIES,
                }
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, t) in defs.iter().enumerate() {
            let entry = add_entry(&t.key, &t.label, pos, &mut by_key, |k| {
                MethodologyError::DuplicateThreatCategory(k)
            })?;
            if t.violates.is_empty() {
                return Err(MethodologyError::EmptyViolates {
                    category: Arc::clone(&t.key),
                });
            }
            let mut violates = PropertySet::empty();
            for key in &t.violates {
                let Some(&p) = property_by_key.get(key) else {
                    return Err(MethodologyError::UnknownViolatedProperty {
                        category: Arc::clone(&t.key),
                        property: Arc::clone(key),
                    });
                };
                if violates.contains(Property(p)) {
                    return Err(MethodologyError::RepeatedViolatedProperty {
                        category: Arc::clone(&t.key),
                        property: Arc::clone(key),
                    });
                }
                violates = violates.with(Property(p));
            }
            out.push(ThreatCategoryData { entry, violates });
        }
        Ok(Vocab { items: out, by_key })
    }

    fn build_schemes(defs: &[CatalogSchemeDef]) -> Result<Vocab<SchemeData>, MethodologyError> {
        check_count(defs.len(), 0, MAX_CATALOG_SCHEMES, || {
            MethodologyError::TooManyCatalogSchemes {
                max: MAX_CATALOG_SCHEMES,
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, s) in defs.iter().enumerate() {
            let entry = add_entry(&s.key, &s.label, pos, &mut by_key, |k| {
                MethodologyError::DuplicateCatalogScheme(k)
            })?;
            s.syntax.check(&s.key)?;
            out.push(SchemeData {
                entry,
                syntax: s.syntax.clone(),
            });
        }
        Ok(Vocab { items: out, by_key })
    }

    // -- Identity ----------------------------------------------------------

    /// Returns the stable key. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.key(), "iso21434-r155");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the definition version. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.version(), "0.1.0");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the display label. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert!(!Methodology::iso21434_r155()?.label().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    // -- Impact categories and levels --------------------------------------

    /// Iterates the impact categories in declaration order. Time O(1) per
    /// item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let keys: Vec<_> = m.impact_categories().filter_map(|c| m.impact_category_key(c)).collect();
    /// assert_eq!(keys, ["safety", "financial", "operational", "privacy"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn impact_categories(&self) -> impl ExactSizeIterator<Item = ImpactCategory> + use<> {
        index_range(self.impact_categories.len()).map(ImpactCategory)
    }

    /// Returns the number of impact categories. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.impact_category_count(), 4);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn impact_category_count(&self) -> usize {
        self.impact_categories.len()
    }

    /// Resolves an impact category key. Time O(log m).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.impact_category("privacy").map(|c| c.index()), Some(3));
    /// assert_eq!(m.impact_category("nope"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn impact_category(&self, key: &str) -> Option<ImpactCategory> {
        self.impact_by_key.get(key).copied().map(ImpactCategory)
    }

    /// Returns an impact category's key, or `None` if the index is not in
    /// this methodology. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let c = m.impact_category("financial");
    /// assert_eq!(c.and_then(|c| m.impact_category_key(c)), Some("financial"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn impact_category_key(&self, cat: ImpactCategory) -> Option<&str> {
        self.category_data(cat).map(|c| &*c.entry.key)
    }

    /// Returns an impact category's label. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let c = m.impact_category("safety");
    /// assert_eq!(c.and_then(|c| m.impact_category_label(c)), Some("Safety"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn impact_category_label(&self, cat: ImpactCategory) -> Option<&str> {
        self.category_data(cat).map(|c| &*c.entry.label)
    }

    /// Iterates the levels of one category's scale, lowest first. Empty if
    /// the category is not in this methodology. Time O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let Some(cat) = m.impact_category("safety") else { return Ok(()) };
    /// let keys: Vec<_> = m.levels(cat).filter_map(|l| m.level_key(cat, l)).collect();
    /// assert_eq!(keys, ["negligible", "moderate", "major", "severe"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn levels(&self, cat: ImpactCategory) -> impl ExactSizeIterator<Item = Level> + use<> {
        index_range(self.level_count(cat)).map(Level)
    }

    /// Returns the number of levels in one category's scale, or 0 if the
    /// category is not in this methodology. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let counts: Vec<_> = m.impact_categories().map(|c| m.level_count(c)).collect();
    /// assert_eq!(counts, [4, 4, 4, 4]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn level_count(&self, cat: ImpactCategory) -> usize {
        self.category_data(cat).map_or(0, |c| c.levels.len())
    }

    /// Resolves a level key within one category. Time O(log m).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let Some(cat) = m.impact_category("safety") else { return Ok(()) };
    /// assert_eq!(m.level(cat, "major").map(|l| l.index()), Some(2));
    /// assert_eq!(m.level(cat, "huge"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn level(&self, cat: ImpactCategory, key: &str) -> Option<Level> {
        self.category_data(cat)
            .and_then(|c| c.level_by_key.get(key).copied())
            .map(Level)
    }

    /// Returns a level's key, or `None` if the level is not in that
    /// category's scale. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let Some(cat) = m.impact_category("safety") else { return Ok(()) };
    /// let top = m.levels(cat).last();
    /// assert_eq!(top.and_then(|l| m.level_key(cat, l)), Some("severe"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn level_key(&self, cat: ImpactCategory, level: Level) -> Option<&str> {
        self.level_entry(cat, level).map(|e| &*e.key)
    }

    /// Returns a level's label. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let Some(cat) = m.impact_category("safety") else { return Ok(()) };
    /// let low = m.levels(cat).next();
    /// assert_eq!(low.and_then(|l| m.level_label(cat, l)), Some("Negligible"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn level_label(&self, cat: ImpactCategory, level: Level) -> Option<&str> {
        self.level_entry(cat, level).map(|e| &*e.label)
    }

    // -- Properties ----------------------------------------------------------

    /// Iterates the security properties in declaration order. Time O(1) per
    /// item.
    ///
    /// ```
    /// use aegis_model::{Methodology, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let all: PropertySet = m.properties().collect();
    /// assert_eq!(all.len(), 6);
    /// # Ok(())
    /// # }
    /// ```
    pub fn properties(&self) -> impl ExactSizeIterator<Item = Property> + use<> {
        index_range(self.properties.len()).map(Property)
    }

    /// Returns the number of security properties. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.property_count(), 6);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn property_count(&self) -> usize {
        self.properties.len()
    }

    /// Resolves a property key. Time O(log m).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.property("availability").is_some());
    /// assert!(m.property("Availability").is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn property(&self, key: &str) -> Option<Property> {
        self.property_by_key.get(key).copied().map(Property)
    }

    /// Returns a property's key. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let first = m.properties().next();
    /// assert_eq!(first.and_then(|p| m.property_key(p)), Some("confidentiality"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn property_key(&self, p: Property) -> Option<&str> {
        self.properties.get(p.index()).map(|e| &*e.key)
    }

    /// Returns a property's label. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let p = m.property("integrity");
    /// assert_eq!(p.and_then(|p| m.property_label(p)), Some("Integrity"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn property_label(&self, p: Property) -> Option<&str> {
        self.properties.get(p.index()).map(|e| &*e.label)
    }

    /// Returns the set of every declared property. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.all_properties().len(), m.property_count());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn all_properties(&self) -> PropertySet {
        self.all_properties
    }

    // -- Threat categories -----------------------------------------------------

    /// Iterates the threat categories in declaration order. Time O(1) per
    /// item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let first = m.threat_categories().next();
    /// assert_eq!(first.and_then(|t| m.threat_category_key(t)), Some("spoofing"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn threat_categories(&self) -> impl ExactSizeIterator<Item = ThreatCategory> + use<> {
        index_range(self.threat_categories.len()).map(ThreatCategory)
    }

    /// Returns the number of threat categories. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.threat_category_count(), 6);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_category_count(&self) -> usize {
        self.threat_categories.len()
    }

    /// Resolves a threat category key. Time O(log m).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.threat_category("tampering").map(|t| t.index()), Some(1));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_category(&self, key: &str) -> Option<ThreatCategory> {
        self.threat_by_key.get(key).copied().map(ThreatCategory)
    }

    /// Returns a threat category's key. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let t = m.threat_category("repudiation");
    /// assert_eq!(t.and_then(|t| m.threat_category_key(t)), Some("repudiation"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_category_key(&self, t: ThreatCategory) -> Option<&str> {
        self.threat_categories.get(t.index()).map(|d| &*d.entry.key)
    }

    /// Returns a threat category's label. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let t = m.threat_category("spoofing");
    /// assert_eq!(t.and_then(|t| m.threat_category_label(t)), Some("Spoofing"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_category_label(&self, t: ThreatCategory) -> Option<&str> {
        self.threat_categories
            .get(t.index())
            .map(|d| &*d.entry.label)
    }

    /// Returns the properties a threat category violates, or the empty set
    /// if the category is not in this methodology. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let (Some(t), Some(i)) = (m.threat_category("tampering"), m.property("integrity")) else {
    ///     return Ok(());
    /// };
    /// assert_eq!(m.violates(t).iter().collect::<Vec<_>>(), [i]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn violates(&self, t: ThreatCategory) -> PropertySet {
        self.threat_categories
            .get(t.index())
            .map_or(PropertySet::empty(), |d| d.violates)
    }

    // -- Catalog schemes -------------------------------------------------------

    /// Iterates the catalog schemes in declaration order. Time O(1) per
    /// item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.catalog_schemes().count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn catalog_schemes(&self) -> impl ExactSizeIterator<Item = CatalogScheme> + use<> {
        index_range(self.schemes.len()).map(CatalogScheme)
    }

    /// Returns the number of catalog schemes. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// assert_eq!(Methodology::iso21434_r155()?.catalog_scheme_count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_scheme_count(&self) -> usize {
        self.schemes.len()
    }

    /// Resolves a catalog scheme key. Time O(log m).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.catalog_scheme("r155-annex5").is_some());
    /// assert!(m.catalog_scheme("capec").is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_scheme(&self, key: &str) -> Option<CatalogScheme> {
        self.scheme_by_key.get(key).copied().map(CatalogScheme)
    }

    /// Returns a catalog scheme's key. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let s = m.catalog_schemes().next();
    /// assert_eq!(s.and_then(|s| m.catalog_scheme_key(s)), Some("r155-annex5"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_scheme_key(&self, s: CatalogScheme) -> Option<&str> {
        self.schemes.get(s.index()).map(|d| &*d.entry.key)
    }

    /// Returns a catalog scheme's label. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let s = m.catalog_schemes().next();
    /// assert_eq!(s.and_then(|s| m.catalog_scheme_label(s)), Some("UN R155 Annex 5"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_scheme_label(&self, s: CatalogScheme) -> Option<&str> {
        self.schemes.get(s.index()).map(|d| &*d.entry.label)
    }

    /// Returns a catalog scheme's ID syntax. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let s = m.catalog_schemes().next();
    /// assert_eq!(s.and_then(|s| m.catalog_syntax(s)).map(|x| x.segments.len()), Some(2));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_syntax(&self, s: CatalogScheme) -> Option<&CatalogSyntax> {
        self.schemes.get(s.index()).map(|d| &d.syntax)
    }

    /// Parses a catalog ID under one of this methodology's schemes.
    ///
    /// Time O(text length).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownCatalogScheme`] if the scheme is not in this
    /// methodology; [`ModelError::InvalidCatalogId`] if the text does not
    /// match the scheme's syntax exactly (see [`CatalogSyntax`]).
    ///
    /// ```
    /// use aegis_model::{Methodology, ModelError};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let scheme = m.catalog_scheme("r155-annex5").ok_or("no scheme")?;
    /// let r = m.parse_catalog_ref(scheme, "4.1")?;
    /// assert_eq!(r.segments(), [4, 1]);
    /// assert_eq!(m.parse_catalog_ref(scheme, "4.0"), Err(ModelError::InvalidCatalogId { scheme }));
    /// # Ok(())
    /// # }
    /// ```
    pub fn parse_catalog_ref(
        &self,
        scheme: CatalogScheme,
        text: &str,
    ) -> Result<CatalogRef, ModelError> {
        let syntax = self
            .catalog_syntax(scheme)
            .ok_or(ModelError::UnknownCatalogScheme(scheme))?;
        syntax.parse(scheme, text)
    }

    /// Formats a catalog reference in its scheme's syntax: the prefix, then
    /// the segments joined by `.`, zero-padded where the syntax fixes the
    /// digit count. If the scheme is not in this methodology only the
    /// segments are written. Time O(1) to build, O(output) to write.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let scheme = m.catalog_scheme("r155-annex5").ok_or("no scheme")?;
    /// let r = m.parse_catalog_ref(scheme, "32.99")?;
    /// assert_eq!(m.display_catalog_ref(&r).to_string(), "32.99");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn display_catalog_ref<'a>(&'a self, r: &CatalogRef) -> impl fmt::Display + use<'a> {
        catalog::CatalogRefDisplay {
            syntax: self.catalog_syntax(r.scheme()),
            r: *r,
        }
    }

    // -- Checks used by the project -------------------------------------------

    /// Checks that a catalog ref names a declared scheme and satisfies its
    /// syntax. Time O(1).
    pub(crate) fn check_catalog_ref(&self, r: &CatalogRef) -> Result<(), ModelError> {
        let syntax = self
            .catalog_syntax(r.scheme())
            .ok_or(ModelError::UnknownCatalogScheme(r.scheme()))?;
        if syntax.accepts(r) {
            Ok(())
        } else {
            Err(ModelError::InvalidCatalogId { scheme: r.scheme() })
        }
    }

    /// Checks that every member of `set` is declared. Time O(1).
    pub(crate) fn check_properties(&self, set: PropertySet) -> Result<(), ModelError> {
        match set.iter().find(|p| !self.all_properties.contains(*p)) {
            Some(p) => Err(ModelError::UnknownProperty(p)),
            None => Ok(()),
        }
    }

    /// Checks that a threat category is declared. Time O(1).
    pub(crate) fn check_threat_category(&self, t: ThreatCategory) -> Result<(), ModelError> {
        if t.index() < self.threat_categories.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownThreatCategory(t))
        }
    }

    /// Checks that an impact category is declared. Time O(1).
    pub(crate) fn check_impact_category(&self, cat: ImpactCategory) -> Result<(), ModelError> {
        if cat.index() < self.impact_categories.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownImpactCategory(cat))
        }
    }

    /// Checks that a level is in a declared category's scale. Time O(1).
    pub(crate) fn check_level(&self, cat: ImpactCategory, level: Level) -> Result<(), ModelError> {
        self.check_impact_category(cat)?;
        if level.index() < self.level_count(cat) {
            Ok(())
        } else {
            Err(ModelError::UnknownLevel {
                category: cat,
                level,
            })
        }
    }

    fn category_data(&self, cat: ImpactCategory) -> Option<&CategoryData> {
        self.impact_categories.get(cat.index())
    }

    fn level_entry(&self, cat: ImpactCategory, level: Level) -> Option<&Entry> {
        self.category_data(cat)
            .and_then(|c| c.levels.get(level.index()))
    }
}

/// `0..len` as `u8` values. Vocabulary sizes are at most 64, checked in
/// [`Methodology::new`], so every position fits.
fn index_range(len: usize) -> impl ExactSizeIterator<Item = u8> {
    let end = u8::try_from(len).unwrap_or(u8::MAX);
    0..end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_def(key: &str) -> PropertyDef {
        PropertyDef {
            key: key.into(),
            label: key.to_uppercase().into(),
        }
    }

    fn level(key: &str) -> LevelDef {
        LevelDef {
            key: key.into(),
            label: key.into(),
        }
    }

    fn category(key: &str, levels: usize) -> ImpactCategoryDef {
        ImpactCategoryDef {
            key: key.into(),
            label: key.into(),
            levels: (0..levels).map(|i| level(&format!("l{i}"))).collect(),
        }
    }

    fn threat(key: &str, violates: &[&str]) -> ThreatCategoryDef {
        ThreatCategoryDef {
            key: key.into(),
            label: key.into(),
            violates: violates.iter().map(|&v| v.into()).collect(),
        }
    }

    fn scheme(
        key: &str,
        prefix: &str,
        segments: Vec<SegmentDef>,
        required: u8,
    ) -> CatalogSchemeDef {
        CatalogSchemeDef {
            key: key.into(),
            label: key.into(),
            syntax: CatalogSyntax {
                prefix: prefix.into(),
                segments,
                required,
            },
        }
    }

    fn seg(min: u16, max: u16, digits: Option<u8>) -> SegmentDef {
        SegmentDef { min, max, digits }
    }

    /// A small valid definition that each test breaks in one way.
    fn base() -> MethodologyDef {
        MethodologyDef {
            key: "base".into(),
            version: "1.0".into(),
            label: "Base".into(),
            impact_categories: vec![category("harm", 3), category("cost", 2)],
            properties: vec![entry_def("c"), entry_def("i"), entry_def("a")],
            threat_categories: vec![threat("leak", &["c"]), threat("wipe", &["i", "a"])],
            catalog_schemes: vec![scheme("cat", "X-", vec![seg(1, 99, None)], 1)],
        }
    }

    fn rejects(def: MethodologyDef, want: &MethodologyError) {
        assert_eq!(Methodology::new(def).as_ref().err(), Some(want));
    }

    #[test]
    fn base_is_valid() {
        let m = Methodology::new(base()).expect("valid");
        assert_eq!(m.impact_category_count(), 2);
        assert_eq!(m.level_count(ImpactCategory(0)), 3);
        assert_eq!(m.property_count(), 3);
        assert_eq!(m.threat_category_count(), 2);
        assert_eq!(m.catalog_scheme_count(), 1);
        assert_eq!(m.violates(ThreatCategory(1)).len(), 2);
        assert_eq!(m.violates(ThreatCategory(9)), PropertySet::empty());
        assert_eq!(m.level_count(ImpactCategory(9)), 0);
        assert_eq!(m.levels(ImpactCategory(9)).count(), 0);
        assert_eq!(m.all_properties().len(), 3);
    }

    #[test]
    fn lookups_round_trip() {
        let m = Methodology::new(base()).expect("valid");
        for c in m.impact_categories() {
            let key = m.impact_category_key(c).expect("key");
            assert_eq!(m.impact_category(key), Some(c));
            assert!(m.impact_category_label(c).is_some());
            for l in m.levels(c) {
                let lk = m.level_key(c, l).expect("level key");
                assert_eq!(m.level(c, lk), Some(l));
                assert!(m.level_label(c, l).is_some());
            }
        }
        for p in m.properties() {
            assert_eq!(m.property(m.property_key(p).expect("key")), Some(p));
            assert!(m.property_label(p).is_some());
        }
        for t in m.threat_categories() {
            assert_eq!(
                m.threat_category(m.threat_category_key(t).expect("k")),
                Some(t)
            );
            assert!(m.threat_category_label(t).is_some());
        }
        for s in m.catalog_schemes() {
            assert_eq!(
                m.catalog_scheme(m.catalog_scheme_key(s).expect("k")),
                Some(s)
            );
            assert!(m.catalog_scheme_label(s).is_some());
            assert!(m.catalog_syntax(s).is_some());
        }
        assert_eq!(m.property_key(Property(3)), None);
        assert_eq!(m.level_key(ImpactCategory(1), Level(2)), None);
        assert_eq!(m.catalog_scheme_key(CatalogScheme(1)), None);
        assert_eq!(m.threat_category_label(ThreatCategory(2)), None);
    }

    #[test]
    fn checks_report_out_of_range_indices() {
        let m = Methodology::new(base()).expect("valid");
        assert_eq!(
            m.check_impact_category(ImpactCategory(2)),
            Err(ModelError::UnknownImpactCategory(ImpactCategory(2)))
        );
        assert_eq!(
            m.check_level(ImpactCategory(1), Level(2)),
            Err(ModelError::UnknownLevel {
                category: ImpactCategory(1),
                level: Level(2)
            })
        );
        assert_eq!(m.check_level(ImpactCategory(0), Level(2)), Ok(()));
        assert_eq!(
            m.check_threat_category(ThreatCategory(2)),
            Err(ModelError::UnknownThreatCategory(ThreatCategory(2)))
        );
        let set = PropertySet::empty().with(Property(0)).with(Property(4));
        assert_eq!(
            m.check_properties(set),
            Err(ModelError::UnknownProperty(Property(4)))
        );
    }

    #[test]
    fn methodology_rejects_duplicate_impact_category() {
        let mut d = base();
        d.impact_categories.push(category("harm", 2));
        rejects(d, &MethodologyError::DuplicateImpactCategory("harm".into()));
    }

    #[test]
    fn methodology_rejects_duplicate_level() {
        let mut d = base();
        d.impact_categories[0].levels.push(level("l0"));
        rejects(
            d,
            &MethodologyError::DuplicateLevel {
                category: "harm".into(),
                level: "l0".into(),
            },
        );
    }

    #[test]
    fn methodology_rejects_duplicate_property() {
        let mut d = base();
        d.properties.push(entry_def("i"));
        rejects(d, &MethodologyError::DuplicateProperty("i".into()));
    }

    #[test]
    fn methodology_rejects_duplicate_threat_category() {
        let mut d = base();
        d.threat_categories.push(threat("leak", &["a"]));
        rejects(d, &MethodologyError::DuplicateThreatCategory("leak".into()));
    }

    #[test]
    fn methodology_rejects_duplicate_catalog_scheme() {
        let mut d = base();
        d.catalog_schemes
            .push(scheme("cat", "Y", vec![seg(1, 9, None)], 1));
        rejects(d, &MethodologyError::DuplicateCatalogScheme("cat".into()));
    }

    #[test]
    fn methodology_rejects_bad_key_syntax() {
        for bad in ["", "Harm", "1harm", "-harm", "ha rm", "harm!", "ünï"] {
            let mut d = base();
            d.impact_categories[0].key = bad.into();
            rejects(d, &MethodologyError::InvalidKey(bad.into()));
        }
        let long: String = std::iter::once('a')
            .chain(std::iter::repeat_n('b', 64))
            .collect();
        let mut d = base();
        d.properties[0].key = long.as_str().into();
        rejects(d, &MethodologyError::InvalidKey(long.as_str().into()));
        // 64 bytes is the limit and passes.
        let ok: String = std::iter::once('a')
            .chain(std::iter::repeat_n('b', 63))
            .collect();
        let mut d = base();
        d.impact_categories[0].key = ok.as_str().into();
        assert!(Methodology::new(d).is_ok());
        // The methodology key and level keys are checked too.
        let mut d = base();
        d.key = "Base".into();
        rejects(d, &MethodologyError::InvalidKey("Base".into()));
        let mut d = base();
        d.impact_categories[1].levels[0].key = "L0".into();
        rejects(d, &MethodologyError::InvalidKey("L0".into()));
    }

    #[test]
    fn methodology_rejects_blank_label() {
        let mut d = base();
        d.properties[1].label = "  \t".into();
        rejects(d, &MethodologyError::BlankLabel { key: "i".into() });
        let mut d = base();
        d.label = String::new().into();
        rejects(d, &MethodologyError::BlankLabel { key: "base".into() });
    }

    #[test]
    fn methodology_rejects_too_long_label() {
        let mut d = base();
        d.threat_categories[0].label = "x".repeat(257).into();
        rejects(
            d,
            &MethodologyError::LabelTooLong {
                key: "leak".into(),
                max: 256,
            },
        );
        let mut d = base();
        d.threat_categories[0].label = "é".repeat(256).into();
        assert!(Methodology::new(d).is_ok(), "256 chars is allowed");
    }

    #[test]
    fn methodology_rejects_blank_version() {
        for bad in ["", "   "] {
            let mut d = base();
            d.version = bad.into();
            rejects(d, &MethodologyError::InvalidVersion);
        }
        let mut d = base();
        d.version = "v".repeat(65).into();
        rejects(d, &MethodologyError::InvalidVersion);
    }

    #[test]
    fn methodology_rejects_one_level_scale() {
        let mut d = base();
        d.impact_categories[1] = category("cost", 1);
        rejects(
            d,
            &MethodologyError::LevelCount {
                category: "cost".into(),
                min: 2,
                max: 16,
            },
        );
    }

    #[test]
    fn methodology_rejects_seventeen_level_scale() {
        let mut d = base();
        d.impact_categories[0] = category("harm", 17);
        rejects(
            d,
            &MethodologyError::LevelCount {
                category: "harm".into(),
                min: 2,
                max: 16,
            },
        );
        let mut d = base();
        d.impact_categories[0] = category("harm", 16);
        assert!(Methodology::new(d).is_ok());
    }

    #[test]
    fn methodology_rejects_impact_category_counts() {
        let mut d = base();
        d.impact_categories.clear();
        rejects(d, &MethodologyError::NoImpactCategory);
        let mut d = base();
        d.impact_categories = (0..33).map(|i| category(&format!("c{i}"), 2)).collect();
        rejects(d, &MethodologyError::TooManyImpactCategories { max: 32 });
        let mut d = base();
        d.impact_categories = (0..32).map(|i| category(&format!("c{i}"), 2)).collect();
        assert!(Methodology::new(d).is_ok());
    }

    #[test]
    fn methodology_rejects_property_counts() {
        let mut d = base();
        d.properties.clear();
        rejects(d, &MethodologyError::NoProperty);
        let mut d = base();
        d.properties = (0..65).map(|i| entry_def(&format!("p{i}"))).collect();
        rejects(d, &MethodologyError::TooManyProperties { max: 64 });
        let mut d = base();
        d.properties = (0..64).map(|i| entry_def(&format!("p{i}"))).collect();
        d.threat_categories = vec![threat("t", &["p63"])];
        let m = Methodology::new(d).expect("64 properties are allowed");
        assert_eq!(m.all_properties().len(), 64);
        assert_eq!(
            m.violates(ThreatCategory(0)).iter().next(),
            Some(Property(63))
        );
    }

    #[test]
    fn methodology_rejects_threat_category_counts() {
        let mut d = base();
        d.threat_categories.clear();
        rejects(d, &MethodologyError::NoThreatCategory);
        let mut d = base();
        d.threat_categories = (0..65).map(|i| threat(&format!("t{i}"), &["c"])).collect();
        rejects(d, &MethodologyError::TooManyThreatCategories { max: 64 });
        let mut d = base();
        d.threat_categories = (0..64).map(|i| threat(&format!("t{i}"), &["c"])).collect();
        assert!(Methodology::new(d).is_ok());
    }

    #[test]
    fn methodology_rejects_seventeen_catalog_schemes() {
        let mut d = base();
        d.catalog_schemes = (0..17)
            .map(|i| scheme(&format!("s{i}"), "", vec![seg(1, 9, None)], 1))
            .collect();
        rejects(d, &MethodologyError::TooManyCatalogSchemes { max: 16 });
        let mut d = base();
        d.catalog_schemes.clear();
        assert!(Methodology::new(d).is_ok(), "zero schemes are allowed");
    }

    #[test]
    fn methodology_rejects_empty_violates() {
        let mut d = base();
        d.threat_categories[1].violates.clear();
        rejects(
            d,
            &MethodologyError::EmptyViolates {
                category: "wipe".into(),
            },
        );
    }

    #[test]
    fn methodology_rejects_unknown_violates() {
        let mut d = base();
        d.threat_categories[1].violates.push("x".into());
        rejects(
            d,
            &MethodologyError::UnknownViolatedProperty {
                category: "wipe".into(),
                property: "x".into(),
            },
        );
    }

    #[test]
    fn methodology_rejects_repeated_violates() {
        let mut d = base();
        d.threat_categories[1].violates.push("i".into());
        rejects(
            d,
            &MethodologyError::RepeatedViolatedProperty {
                category: "wipe".into(),
                property: "i".into(),
            },
        );
    }

    fn with_syntax(prefix: &str, segments: Vec<SegmentDef>, required: u8) -> MethodologyDef {
        let mut d = base();
        d.catalog_schemes = vec![scheme("cat", prefix, segments, required)];
        d
    }

    #[test]
    fn methodology_rejects_catalog_segment_counts() {
        rejects(
            with_syntax("", vec![], 1),
            &MethodologyError::CatalogSegmentCount {
                scheme: "cat".into(),
            },
        );
        rejects(
            with_syntax("", vec![seg(1, 9, None); 5], 1),
            &MethodologyError::CatalogSegmentCount {
                scheme: "cat".into(),
            },
        );
        assert!(Methodology::new(with_syntax("", vec![seg(1, 9, None); 4], 4)).is_ok());
    }

    #[test]
    fn methodology_rejects_catalog_required() {
        let want = MethodologyError::CatalogRequired {
            scheme: "cat".into(),
        };
        rejects(with_syntax("", vec![seg(1, 9, None); 2], 0), &want);
        rejects(with_syntax("", vec![seg(1, 9, None); 2], 3), &want);
    }

    #[test]
    fn methodology_rejects_catalog_min_above_max() {
        rejects(
            with_syntax("", vec![seg(1, 9, None), seg(5, 4, None)], 1),
            &MethodologyError::CatalogSegmentRange {
                scheme: "cat".into(),
                segment: 1,
            },
        );
    }

    #[test]
    fn methodology_rejects_catalog_digits() {
        let want = MethodologyError::CatalogSegmentDigits {
            scheme: "cat".into(),
            segment: 0,
        };
        // Too small for max.
        rejects(with_syntax("T", vec![seg(1, 1000, Some(3))], 1), &want);
        // Out of 1..=5.
        rejects(with_syntax("T", vec![seg(0, 0, Some(0))], 1), &want);
        rejects(with_syntax("T", vec![seg(1, 9, Some(6))], 1), &want);
        assert!(Methodology::new(with_syntax("T", vec![seg(1, 999, Some(3))], 1)).is_ok());
        assert!(Methodology::new(with_syntax("T", vec![seg(0, 65535, Some(5))], 1)).is_ok());
    }

    #[test]
    fn methodology_rejects_catalog_prefix() {
        let want = MethodologyError::CatalogPrefix {
            scheme: "cat".into(),
        };
        for bad in ["T1", "AB C", " T", "X.", "ÄB", "ABCDEFGHIJKLMNOPQ"] {
            rejects(with_syntax(bad, vec![seg(1, 9, None)], 1), &want);
        }
        for ok in ["", "CAPEC-", "a_b", "ABCDEFGHIJKLMNOP"] {
            assert!(Methodology::new(with_syntax(ok, vec![seg(1, 9, None)], 1)).is_ok());
        }
    }

    #[test]
    fn value_sizes() {
        use std::mem::size_of;
        assert_eq!(size_of::<ImpactCategory>(), 1);
        assert_eq!(size_of::<Level>(), 1);
        assert_eq!(size_of::<Property>(), 1);
        assert_eq!(size_of::<ThreatCategory>(), 1);
        assert_eq!(size_of::<CatalogScheme>(), 1);
        assert_eq!(size_of::<PropertySet>(), 8);
        assert!(size_of::<CatalogRef>() <= 12, "{}", size_of::<CatalogRef>());
    }
}
