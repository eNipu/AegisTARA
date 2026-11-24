//! Methodology definitions: the vocabularies a project is checked against.
//!
//! The core model names no impact category, rating scale, feasibility
//! scale, security property, threat taxonomy, or catalog. A
//! [`MethodologyDef`] declares them as plain data (the shape a pack file
//! has, see [`MethodologyDef::read_pack`]), and [`Methodology::new`]
//! validates it once. The validated [`Methodology`] hands out small typed
//! indices ([`ImpactCategory`], [`Level`], [`FeasibilityLevel`],
//! [`Property`], [`ThreatCategory`], [`CatalogScheme`]) that ops and
//! entities carry. Text keys are resolved to indices once, at the boundary.

mod catalog;
mod cra;
mod iso21434;
mod pack;
mod stride;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

pub use catalog::{CatalogRef, CatalogSyntax, SegmentDef};
pub(crate) use catalog::{MAX_PREFIX_LEN, MAX_SEGMENTS};

use crate::asset::PropertySet;
use crate::control::{ControlRule, MAX_CONTROL_RULES};
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
/// Most interface kinds a methodology may declare for the item definition.
pub const MAX_INTERFACE_KINDS: usize = 32;
/// Most attack types a methodology may declare for attack steps (CFG-02).
pub const MAX_ATTACK_TYPES: usize = 32;
/// Most setting presets (control kinds) a methodology may declare (CFG-06).
pub const MAX_CONTROL_KINDS: usize = 64;
/// Most per-language translations a methodology pack may declare.
pub const MAX_PACK_TRANSLATIONS: usize = 16;
/// Most clause refs a pack may list for one page, and the most pages it may list refs for.
pub const MAX_PACK_CLAUSE_REFS: usize = 32;
/// Fewest factors an `attack_potential` scheme may declare.
pub const MIN_FEASIBILITY_FACTORS: usize = 1;
/// Most factors an `attack_potential` scheme may declare.
pub const MAX_FEASIBILITY_FACTORS: usize = 16;
/// Highest point value a factor level may declare.
pub const MAX_FACTOR_POINTS: u16 = 1_000;
/// Longest label, in chars.
pub const MAX_LABEL_CHARS: usize = 256;
/// Longest version string, in chars.
pub const MAX_VERSION_CHARS: usize = 64;
/// Longest key, in bytes (keys are ASCII).
pub const MAX_KEY_LEN: usize = 64;

// ---------------------------------------------------------------------------
// Definition: plain data a pack provides.
// ---------------------------------------------------------------------------

/// A methodology as plain data, before validation.
///
/// All fields are public so tests and tools can build it;
/// [`MethodologyDef::read_pack`] decodes it from a pack file.
/// [`Methodology::new`] checks it. Declaration order is meaningful: it fixes
/// the indices and the iteration order, and a scale's levels are listed
/// lowest first.
///
/// ```
/// use aegis_model::{
///     ElementNamesDef, ImpactCategoryDef, LevelDef, Methodology, MethodologyDef, PropertyDef,
///     ThreatCategoryDef,
/// };
///
/// # fn main() -> Result<(), aegis_model::MethodologyError> {
/// let def = MethodologyDef {
///     key: "tiny".into(),
///     version: "1".into(),
///     label: "Tiny".into(),
///     element_names: ElementNamesDef::default(),
///     impact_categories: vec![ImpactCategoryDef {
///         key: "harm".into(),
///         label: "Harm".into(),
///         levels: vec![
///             LevelDef { key: "low".into(), label: "Low".into() },
///             LevelDef { key: "high".into(), label: "High".into() },
///         ],
///         risk_matrix: vec![
///             vec!["r1".into(), "r2".into()],
///             vec!["r2".into(), "r2".into()],
///         ],
///     }],
///     feasibility_levels: vec![
///         LevelDef { key: "low".into(), label: "Low".into() },
///         LevelDef { key: "high".into(), label: "High".into() },
///     ],
///     risk_levels: vec![
///         LevelDef { key: "r1".into(), label: "Risk 1".into() },
///         LevelDef { key: "r2".into(), label: "Risk 2".into() },
///     ],
///     properties: vec![PropertyDef { key: "integrity".into(), label: "Integrity".into() }],
///     threat_categories: vec![ThreatCategoryDef {
///         key: "corruption".into(),
///         label: "Corruption".into(),
///         violates: vec!["integrity".into()],
///     }],
///     catalog_schemes: vec![],
///     require_rationale: false,
///     clause_refs: std::collections::BTreeMap::default(),
///     attack_potential: None,
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
    /// Pack-defined display names for the model's element types and the
    /// feasibility or likelihood scale (DOM-01, DOM-02).
    pub element_names: ElementNamesDef,
    /// Impact categories, 1 to 32.
    pub impact_categories: Vec<ImpactCategoryDef>,
    /// Ordered scale of attack feasibility or likelihood levels, lowest
    /// first. Must hold between [`MIN_LEVELS`] (2) and [`MAX_LEVELS`] (16)
    /// levels with unique keys.
    pub feasibility_levels: Vec<LevelDef>,
    /// Ordered scale of risk levels, lowest risk first. Must hold between
    /// [`MIN_LEVELS`] (2) and [`MAX_LEVELS`] (16) levels with unique keys.
    pub risk_levels: Vec<LevelDef>,
    /// Security properties, 1 to 64.
    pub properties: Vec<PropertyDef>,
    /// Threat categories, 1 to 64.
    pub threat_categories: Vec<ThreatCategoryDef>,
    /// Catalog schemes, 0 to 16.
    pub catalog_schemes: Vec<CatalogSchemeDef>,
    /// Optional multi-factor attack potential scheme, mapping per-factor
    /// ratings on an attack step to a [`FeasibilityLevel`]. When `None`,
    /// steps and threats use the plain direct-rating scale in
    /// [`feasibility_levels`](Self::feasibility_levels) (DOM-02).
    pub attack_potential: Option<AttackPotentialDef>,
    /// The pack asks for a rationale next to each impact and direct
    /// feasibility rating (FND-09): a rating without one is a lint warning.
    /// Off unless the pack sets `"require_rationale": true`.
    pub require_rationale: bool,
    /// Optional ISO/CRA clause references keyed by page key (for example
    /// `"assets"` or `"threats"` -> `["ISO 21434 RQ-09-01", "CRA Annex I 1.1"]`).
    /// A page key is a value of `Page::key()` in `aegis-client`
    /// (`crates/aegis-client/src/view.rs`). Empty unless the pack sets
    /// `"clause_refs"`.
    pub clause_refs: BTreeMap<Arc<str>, Vec<Arc<str>>>,
}

/// Singular and plural display names for one element kind in a [`MethodologyDef`].
///
/// ```
/// use aegis_model::ElementNameDef;
///
/// let n = ElementNameDef::new("Harm", "Harms");
/// assert_eq!(&*n.singular, "Harm");
/// assert_eq!(&*n.plural, "Harms");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementNameDef {
    /// Singular display label, for example `"Damage scenario"` or `"Harm"`.
    pub singular: Arc<str>,
    /// Plural display label, for example `"Damage scenarios"` or `"Harms"`.
    pub plural: Arc<str>,
}

impl ElementNameDef {
    /// Builds a singular and plural display name pair.
    #[must_use]
    pub fn new(singular: impl Into<Arc<str>>, plural: impl Into<Arc<str>>) -> Self {
        Self {
            singular: singular.into(),
            plural: plural.into(),
        }
    }
}

/// Per-language localized label overrides for a [`MethodologyDef`] (L10N-07).
///
/// Keyed by language code (for example `"ja"`) inside
/// [`ElementNamesDef::translations`]. Any omitted field or key falls back to
/// the methodology's primary labels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackTranslationDef {
    /// Localized display label for the methodology pack itself.
    pub label: Option<Arc<str>>,
    /// Localized display names for element types and the feasibility scale.
    pub element_names: Option<ElementNamesDef>,
    /// Localized impact category labels, keyed by category key.
    pub impact_categories: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized impact level labels, keyed by level key (or `"category.level"`).
    pub impact_levels: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized feasibility level labels, keyed by feasibility level key.
    pub feasibility_levels: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized risk level labels, keyed by risk level key.
    pub risk_levels: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized security property labels, keyed by property key.
    pub properties: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized threat category labels, keyed by threat category key.
    pub threat_categories: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized catalog scheme labels, keyed by catalog scheme key.
    pub catalog_schemes: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized attack potential factor labels, keyed by factor key.
    pub feasibility_factors: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized attack potential factor level labels, keyed by level key (or `"factor.level"`).
    pub factor_levels: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized interface kind labels, keyed by interface kind key.
    pub interface_kinds: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized attack type labels, keyed by attack type key.
    pub attack_types: BTreeMap<Arc<str>, Arc<str>>,
    /// Localized control kind labels, keyed by control kind key.
    pub control_kinds: BTreeMap<Arc<str>, Arc<str>>,
}

/// A setting preset (control kind) in a [`MethodologyDef`]: a named set of
/// default rules a consultant picks instead of typing them (CFG-06).
///
/// ```
/// use aegis_model::ControlKindDef;
///
/// let d = ControlKindDef {
///     key: "secure-boot".into(),
///     label: "Secure boot".into(),
///     interface_kinds: Vec::new(),
///     rules: [("firmware-modification".into(), "very-low".into())].into(),
///     conflicts_with: Vec::new(),
/// };
/// assert!(d.interface_kinds.is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlKindDef {
    /// Stable key, unique among control kinds.
    pub key: Arc<str>,
    /// Display label, used as the name of a setting created from the preset.
    pub label: Arc<str>,
    /// Interface kind keys the preset is for. Empty: the preset is for components.
    pub interface_kinds: Vec<Arc<str>>,
    /// Default rules: attack type key to feasibility level key. 1 to 16 entries.
    pub rules: BTreeMap<Arc<str>, Arc<str>>,
    /// Keys of presets this one conflicts with; empty for none. One-sided
    /// declaration is enough: the lint treats it symmetrically (CFG-09).
    pub conflicts_with: Vec<Arc<str>>,
}

/// Pack-defined display names for the model's element types and the
/// feasibility or likelihood scale (DOM-01, DOM-02, Decision D24, L10N-07,
/// ARC-02).
///
/// [`ElementNamesDef::default`] gives the ISO/SAE 21434 terminology so
/// existing packs that omit `element_names` keep today's names unchanged.
///
/// ```
/// use aegis_model::ElementNamesDef;
///
/// let names = ElementNamesDef::default();
/// assert_eq!(&*names.damage_scenario.singular, "Damage scenario");
/// assert_eq!(&*names.damage_scenario.plural, "Damage scenarios");
/// assert_eq!(&*names.feasibility, "Attack feasibility");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementNamesDef {
    /// Display names for item definition components (default: `"Component"` / `"Components"`).
    pub component: ElementNameDef,
    /// Display names for item definition interfaces (default: `"Interface"` / `"Interfaces"`).
    pub interface: ElementNameDef,
    /// Display names for item definition data flows (default: `"Data flow"` / `"Data flows"`).
    pub data_flow: ElementNameDef,
    /// Display names for item definition trust boundaries (default: `"Trust boundary"` / `"Trust boundaries"`).
    pub trust_boundary: ElementNameDef,
    /// Display names for assets (default: `"Asset"` / `"Assets"`).
    pub asset: ElementNameDef,
    /// Display names for damage scenarios (default: `"Damage scenario"` / `"Damage scenarios"`).
    pub damage_scenario: ElementNameDef,
    /// Display names for threat scenarios (default: `"Threat scenario"` / `"Threat scenarios"`).
    pub threat_scenario: ElementNameDef,
    /// Display names for attack trees (default: `"Attack tree"` / `"Attack trees"`).
    pub attack_tree: ElementNameDef,
    /// Display names for security controls (default: `"Security control"` / `"Security controls"`).
    pub security_control: ElementNameDef,
    /// Display names for cybersecurity goals (default: `"Cybersecurity goal"` / `"Cybersecurity goals"`).
    pub cybersecurity_goal: ElementNameDef,
    /// Display names for cybersecurity claims (default: `"Cybersecurity claim"` / `"Cybersecurity claims"`).
    pub cybersecurity_claim: ElementNameDef,
    /// Display label for the feasibility or likelihood scale (default: `"Attack feasibility"`).
    pub feasibility: Arc<str>,
    /// Pack-defined interface kinds for the item definition, 0 to 32 (ARC-02, Decision D15).
    pub interface_kinds: Vec<LevelDef>,
    /// Pack-defined attack types for attack steps, 0 to 32 (CFG-02, Decision D26).
    pub attack_types: Vec<LevelDef>,
    /// Pack-defined setting presets, 0 to 64 (CFG-06, Decision D26).
    pub control_kinds: Vec<ControlKindDef>,
    /// Optional per-language translations of pack labels and element names (L10N-07).
    pub translations: BTreeMap<Arc<str>, PackTranslationDef>,
}

impl ElementNamesDef {
    /// Returns `true` if any of the element type display names or the
    /// feasibility scale label differ from the built-in English defaults.
    #[must_use]
    pub fn has_custom_names(&self) -> bool {
        let def = Self::default();
        self.component != def.component
            || self.interface != def.interface
            || self.data_flow != def.data_flow
            || self.trust_boundary != def.trust_boundary
            || self.asset != def.asset
            || self.damage_scenario != def.damage_scenario
            || self.threat_scenario != def.threat_scenario
            || self.attack_tree != def.attack_tree
            || self.security_control != def.security_control
            || self.cybersecurity_goal != def.cybersecurity_goal
            || self.cybersecurity_claim != def.cybersecurity_claim
            || self.feasibility != def.feasibility
    }

    /// Returns `true` if any of the 4 item-definition element names
    /// (`component`, `interface`, `data_flow`, `trust_boundary`) differ from
    /// their defaults.
    #[must_use]
    pub fn has_custom_item_names(&self) -> bool {
        let def = Self::default();
        self.component != def.component
            || self.interface != def.interface
            || self.data_flow != def.data_flow
            || self.trust_boundary != def.trust_boundary
    }
}

impl Default for ElementNamesDef {
    fn default() -> Self {
        Self {
            component: ElementNameDef::new("Component", "Components"),
            interface: ElementNameDef::new("Interface", "Interfaces"),
            data_flow: ElementNameDef::new("Data flow", "Data flows"),
            trust_boundary: ElementNameDef::new("Trust boundary", "Trust boundaries"),
            asset: ElementNameDef::new("Asset", "Assets"),
            damage_scenario: ElementNameDef::new("Damage scenario", "Damage scenarios"),
            threat_scenario: ElementNameDef::new("Threat scenario", "Threat scenarios"),
            attack_tree: ElementNameDef::new("Attack tree", "Attack trees"),
            security_control: ElementNameDef::new("Security control", "Security controls"),
            cybersecurity_goal: ElementNameDef::new("Cybersecurity goal", "Cybersecurity goals"),
            cybersecurity_claim: ElementNameDef::new("Cybersecurity claim", "Cybersecurity claims"),
            feasibility: Arc::from("Attack feasibility"),
            interface_kinds: Vec::new(),
            attack_types: Vec::new(),
            control_kinds: Vec::new(),
            translations: BTreeMap::new(),
        }
    }
}

/// One impact category, its ordered rating scale, and its 2D risk matrix.
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
///     risk_matrix: vec![
///         vec!["r1".into(), "r2".into()],
///         vec!["r2".into(), "r2".into()],
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
    /// 2D lookup table mapping `(impact_level, feasibility_level)` to a key
    /// in [`MethodologyDef::risk_levels`]. Must have `levels.len()` rows and
    /// `feasibility_levels.len()` columns per row.
    pub risk_matrix: Vec<Vec<Arc<str>>>,
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

/// A methodology's multi-factor attack potential scheme: the factors rated
/// on an attack step and the score bands that map the point sum to a
/// [`FeasibilityLevel`].
///
/// ```
/// use aegis_model::{AttackPotentialDef, FactorDef, FactorLevelDef, PotentialBandDef};
///
/// let ap = AttackPotentialDef {
///     factors: vec![FactorDef {
///         key: "skill".into(),
///         label: "Skill".into(),
///         levels: vec![
///             FactorLevelDef { key: "low".into(), label: "Low".into(), points: 0 },
///             FactorLevelDef { key: "high".into(), label: "High".into(), points: 5 },
///         ],
///     }],
///     bands: vec![
///         PotentialBandDef { min: 0, max: Some(4), feasibility: "high".into() },
///         PotentialBandDef { min: 5, max: None, feasibility: "low".into() },
///     ],
/// };
/// assert_eq!(ap.factors.len(), 1);
/// assert_eq!(ap.bands.len(), 2);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackPotentialDef {
    /// Factors rated on each attack step, 1 to 16.
    pub factors: Vec<FactorDef>,
    /// Contiguous score bands partitioning `0..`, 1 to 16.
    pub bands: Vec<PotentialBandDef>,
}

/// One factor in an [`AttackPotentialDef`] and its ordered rating scale.
///
/// ```
/// use aegis_model::{FactorDef, FactorLevelDef};
///
/// let f = FactorDef {
///     key: "skill".into(),
///     label: "Skill".into(),
///     levels: vec![
///         FactorLevelDef { key: "low".into(), label: "Low".into(), points: 0 },
///         FactorLevelDef { key: "high".into(), label: "High".into(), points: 5 },
///     ],
/// };
/// assert_eq!(&*f.key, "skill");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorDef {
    /// Stable key, unique among attack potential factors.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
    /// Ordered scale of factor levels, 2 to 16.
    pub levels: Vec<FactorLevelDef>,
}

/// One choice in a [`FactorDef`]'s rating scale and its point contribution.
///
/// ```
/// use aegis_model::FactorLevelDef;
///
/// let fl = FactorLevelDef { key: "expert".into(), label: "Expert".into(), points: 6 };
/// assert_eq!(fl.points, 6);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorLevelDef {
    /// Stable key, unique within its factor.
    pub key: Arc<str>,
    /// Display label.
    pub label: Arc<str>,
    /// Point contribution toward total attack potential, at most
    /// [`MAX_FACTOR_POINTS`].
    pub points: u16,
}

/// One score band in an [`AttackPotentialDef`], mapping an inclusive point
/// range `[min, max]` (or `[min, ∞)` for the final band) to a key in
/// [`MethodologyDef::feasibility_levels`].
///
/// ```
/// use aegis_model::PotentialBandDef;
///
/// let b = PotentialBandDef { min: 0, max: Some(13), feasibility: "high".into() };
/// assert_eq!(b.max, Some(13));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PotentialBandDef {
    /// Inclusive lower bound of the band's score range.
    pub min: u16,
    /// Inclusive upper bound of the band's score range, or `None` for the
    /// final open-ended band (`score >= min`).
    pub max: Option<u16>,
    /// Key of the [`FeasibilityLevel`] assigned to scores in this band.
    pub feasibility: Arc<str>,
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
            /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let cat = m.impact_category("safety").ok_or(aegis_model::MethodologyError::NoImpactCategory)?;
    /// assert!(m.level(cat, "negligible") < m.level(cat, "severe"));
    /// # Ok(())
    /// # }
    /// ```
    Level
}

vocab_index! {
    /// Index of a feasibility level inside a [`Methodology`], ordered from
    /// lowest (`0`) to highest feasibility. 1 byte.
    ///
    /// `Ord` is the scale order (lowest feasibility = 0), so
    /// [`MaxMin<FeasibilityLevel>`](crate::MaxMin) evaluates `OR` gates as
    /// `max` (most feasible child) and `AND` / `SeqAnd` gates as `min`
    /// (least feasible bottleneck child).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.feasibility_level("very-low") < m.feasibility_level("high"));
    /// # Ok(())
    /// # }
    /// ```
    FeasibilityLevel
}

vocab_index! {
    /// Index of a risk level inside a [`Methodology`], ordered from lowest
    /// (`0`) to highest risk. 1 byte.
    ///
    /// `Ord` is the scale order (lowest risk = 0), so worst-case risk across
    /// impact categories or damage scenarios is simply `max`.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.risk_level("r1") < m.risk_level("r5"));
    /// # Ok(())
    /// # }
    /// ```
    RiskLevel
}

vocab_index! {
    /// Index of a security property in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.catalog_scheme("r155-annex5").map(|s| s.index()), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    CatalogScheme
}

vocab_index! {
    /// Index of an attack potential factor in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.feasibility_factor("expertise").map(|f| f.index()), Some(1));
    /// # Ok(())
    /// # }
    /// ```
    FeasibilityFactor
}

vocab_index! {
    /// A position in one attack potential factor's rating scale. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let exp = m.feasibility_factor("expertise").ok_or("no factor")?;
    /// assert_eq!(m.factor_level(exp, "expert").map(|l| l.index()), Some(2));
    /// # Ok(())
    /// # }
    /// ```
    FactorLevel
}

vocab_index! {
    /// Index of an item definition interface kind in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.interface_kind("can").map(|k| k.index()), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    InterfaceKind
}

vocab_index! {
    /// Index of an attack type in a [`Methodology`]: what kind of attack an
    /// attack step is (firmware modification, debug access, ...). 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.attack_type("firmware-modification").map(|k| k.index()), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    AttackType
}

vocab_index! {
    /// Index of a setting preset (control kind) in a [`Methodology`]. 1 byte.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.control_kind("no-such-preset"), None);
    /// for (i, k) in m.control_kinds().enumerate() {
    ///     assert_eq!(k.index(), i);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ControlKind
}

/// One validated setting preset: its entry, interface kinds and rules.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ControlKindData {
    entry: Entry,
    interface_kinds: Vec<InterfaceKind>,
    rules: Vec<ControlRule>,
    conflicts_with: Vec<ControlKind>,
}

/// One validated score band in a [`Methodology`]'s attack potential scheme.
///
/// ```
/// use aegis_model::Methodology;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let band = m.potential_band_for_score(15).ok_or("no band")?;
/// assert_eq!(band.min(), 14);
/// assert_eq!(band.max(), Some(19));
/// assert!(band.contains(15));
/// assert_eq!(m.feasibility_level_key(band.feasibility()), Some("medium"));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PotentialBand {
    min: u16,
    max: Option<u16>,
    feasibility: FeasibilityLevel,
}

impl PotentialBand {
    /// Returns the inclusive lower bound of this band's score range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.potential_bands().first().map(|b| b.min()), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn min(self) -> u16 {
        self.min
    }

    /// Returns the inclusive upper bound of this band's score range, or `None`
    /// for the final open-ended band. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.potential_bands().last().and_then(|b| b.max()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn max(self) -> Option<u16> {
        self.max
    }

    /// Returns the [`FeasibilityLevel`] assigned to scores in this band. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let first = m.potential_bands().first().ok_or("no band")?;
    /// assert_eq!(m.feasibility_level_key(first.feasibility()), Some("high"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility(self) -> FeasibilityLevel {
        self.feasibility
    }

    /// Returns `true` if `score` falls within `[min, max]` (or `>= min` when
    /// `max` is `None`). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let first = m.potential_bands().first().ok_or("no band")?;
    /// assert!(first.contains(0));
    /// assert!(first.contains(13));
    /// assert!(!first.contains(14));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn contains(self, score: u32) -> bool {
        score >= u32::from(self.min) && self.max.is_none_or(|m| score <= u32::from(m))
    }
}

/// One factor's contribution to an attack step's [`AttackPotential`].
///
/// ```
/// use aegis_model::Methodology;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let ratings: Vec<_> = m
///     .feasibility_factors()
///     .map(|f| m.factor_levels(f).next())
///     .collect();
/// let ap = m.resolve_attack_potential(&ratings).ok_or("no potential")?;
/// let first = ap.factors().first().ok_or("no factor")?;
/// assert_eq!(first.factor().index(), 0);
/// assert_eq!(first.level().index(), 0);
/// assert_eq!(first.points(), 0);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FactorContribution {
    factor: FeasibilityFactor,
    level: FactorLevel,
    points: u16,
}

impl FactorContribution {
    /// Returns the rated [`FeasibilityFactor`]. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// assert_eq!(m.feasibility_factor_key(ap.factors()[0].factor()), Some("elapsed-time"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn factor(&self) -> FeasibilityFactor {
        self.factor
    }

    /// Returns the chosen [`FactorLevel`]. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// let c = ap.factors()[0];
    /// assert_eq!(m.factor_level_key(c.factor(), c.level()), Some("le-1d"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn level(&self) -> FactorLevel {
        self.level
    }

    /// Returns the point contribution of this factor rating. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).last())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// assert_eq!(ap.factors()[0].points(), 19);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn points(&self) -> u16 {
        self.points
    }
}

/// Full breakdown of an attack step's multi-factor attack potential rating:
/// each factor's rated level and point contribution, the summed score, the
/// matched [`PotentialBand`], and the derived [`FeasibilityLevel`].
///
/// ```
/// use aegis_model::Methodology;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let ratings: Vec<_> = m
///     .feasibility_factors()
///     .map(|f| m.factor_levels(f).last())
///     .collect();
/// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
/// assert_eq!(ap.score(), 57);
/// assert_eq!(m.feasibility_level_key(ap.feasibility()), Some("very-low"));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackPotential {
    factors: Box<[FactorContribution]>,
    score: u32,
    band: PotentialBand,
    feasibility: FeasibilityLevel,
}

impl AttackPotential {
    /// Returns each factor's rating and point contribution in methodology
    /// declaration order. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// assert_eq!(ap.factors().len(), 5);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn factors(&self) -> &[FactorContribution] {
        &self.factors
    }

    /// Returns the total attack potential score (sum of factor points). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// assert_eq!(ap.score(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn score(&self) -> u32 {
        self.score
    }

    /// Returns the [`PotentialBand`] matched by [`score`](Self::score). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// assert_eq!(ap.band().max(), Some(13));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn band(&self) -> PotentialBand {
        self.band
    }

    /// Returns the [`FeasibilityLevel`] derived from [`band`](Self::band). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("ap")?;
    /// assert_eq!(m.feasibility_level_key(ap.feasibility()), Some("high"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn feasibility(&self) -> FeasibilityLevel {
        self.feasibility
    }
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

impl Level {
    /// Wraps a raw position (used by sensitivity analysis and tests).
    pub(crate) fn new(i: u8) -> Self {
        Level(i)
    }
}

impl FeasibilityLevel {
    /// Wraps a raw position (used by sensitivity analysis).
    pub(crate) fn new(i: u8) -> Self {
        FeasibilityLevel(i)
    }
}

#[cfg(test)]
impl CatalogScheme {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        CatalogScheme(i)
    }
}

#[cfg(test)]
impl FeasibilityFactor {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        FeasibilityFactor(i)
    }
}

#[cfg(test)]
impl FactorLevel {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        FactorLevel(i)
    }
}

#[cfg(test)]
impl AttackType {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        AttackType(i)
    }
}

#[cfg(test)]
impl InterfaceKind {
    /// Wraps a raw position, for tests.
    pub(crate) fn new(i: u8) -> Self {
        InterfaceKind(i)
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

/// One validated factor level.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FactorLevelData {
    entry: Entry,
    points: u16,
}

/// One validated attack potential factor.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FactorData {
    entry: Entry,
    levels: Vec<FactorLevelData>,
    level_by_key: BTreeMap<Arc<str>, u8>,
}

/// A validated impact category.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CategoryData {
    entry: Entry,
    levels: Vec<Entry>,
    level_by_key: BTreeMap<Arc<str>, u8>,
    /// Row-major `levels.len() * feasibility_levels.len()` lookup table.
    risk_matrix: Box<[RiskLevel]>,
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
/// Built only by [`Methodology::new`] (or by [`Methodology::read_pack`] and
/// the built-in [`Methodology::iso21434_r155`], which go through `new`). It hands out
/// typed vocabulary indices, resolves keys to indices in O(log m) (m = the
/// vocabulary size), and parses and formats catalog IDs.
///
/// ```
/// use aegis_model::Methodology;
///
/// # fn main() -> Result<(), aegis_model::PackError> {
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
    element_names: ElementNamesDef,
    interface_kinds: Vec<Entry>,
    interface_by_key: BTreeMap<Arc<str>, u8>,
    attack_types: Vec<Entry>,
    attack_by_key: BTreeMap<Arc<str>, u8>,
    control_kinds: Vec<ControlKindData>,
    control_kind_by_key: BTreeMap<Arc<str>, u8>,
    translations: BTreeMap<Arc<str>, PackTranslationDef>,
    impact_categories: Vec<CategoryData>,
    impact_by_key: BTreeMap<Arc<str>, u8>,
    feasibility_levels: Vec<Entry>,
    feasibility_by_key: BTreeMap<Arc<str>, u8>,
    risk_levels: Vec<Entry>,
    risk_by_key: BTreeMap<Arc<str>, u8>,
    properties: Vec<Entry>,
    property_by_key: BTreeMap<Arc<str>, u8>,
    threat_categories: Vec<ThreatCategoryData>,
    threat_by_key: BTreeMap<Arc<str>, u8>,
    schemes: Vec<SchemeData>,
    scheme_by_key: BTreeMap<Arc<str>, u8>,
    factors: Vec<FactorData>,
    factor_by_key: BTreeMap<Arc<str>, u8>,
    potential_bands: Vec<PotentialBand>,
    require_rationale: bool,
    clause_refs: BTreeMap<Arc<str>, Vec<Arc<str>>>,
    /// Every declared property.
    all_properties: PropertySet,
    /// Immutable content digest (`sha256:<hex>`) of the canonical pack JSON.
    pack_digest: Arc<str>,
}

/// Computes `sha256:<64 lowercase hex>` over the canonical JSON encoding of
/// `def`, sorting unordered vocabulary collections by key so declaration order
/// does not alter the digest while every semantic rule (matrices, level order,
/// attack potential bands, and keys) is pinned.
fn pack_digest_of(def: &MethodologyDef) -> Arc<str> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let mut normalized = def.clone();
    normalized.properties.sort_by(|a, b| a.key.cmp(&b.key));
    normalized
        .threat_categories
        .sort_by(|a, b| a.key.cmp(&b.key));
    normalized.catalog_schemes.sort_by(|a, b| a.key.cmp(&b.key));
    normalized
        .impact_categories
        .sort_by(|a, b| a.key.cmp(&b.key));
    normalized
        .element_names
        .interface_kinds
        .sort_by(|a, b| a.key.cmp(&b.key));
    normalized
        .element_names
        .attack_types
        .sort_by(|a, b| a.key.cmp(&b.key));
    normalized
        .element_names
        .control_kinds
        .sort_by(|a, b| a.key.cmp(&b.key));

    let mut canonical = Vec::new();
    // Serializing an in-memory `MethodologyDef` to a `Vec<u8>` cannot fail
    // for valid definitions; fall back to empty bytes only if unreachable.
    let _ = normalized.write_pack(&mut canonical);
    let hash = Sha256::digest(&canonical);
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in hash {
        let _ = write!(out, "{b:02x}");
    }
    Arc::from(out)
}

/// Checks `[a-z][a-z0-9_-]{0,63}`. The length is checked first, so an
/// oversized key is rejected in O(1); otherwise time O(key length) <= O(64).
fn check_key(key: &Arc<str>) -> Result<(), MethodologyError> {
    let bytes = key.as_bytes();
    if bytes.len() > MAX_KEY_LEN {
        return Err(MethodologyError::InvalidKey(error_key_text(key)));
    }
    let first_ok = bytes.first().is_some_and(u8::is_ascii_lowercase);
    let rest_ok = bytes
        .iter()
        .skip(1)
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-');
    if first_ok && rest_ok {
        Ok(())
    } else {
        Err(MethodologyError::InvalidKey(Arc::clone(key)))
    }
}

/// Marker appended to key text cut for an error. `.` is not a key char, so
/// a cut text never looks like a valid key.
const CUT_MARK: &str = "...";

/// The key text an error keeps of a key that may not be valid: at most
/// [`MAX_KEY_LEN`] bytes. A key of at most 64 bytes is shared as is (no
/// allocation); a longer one becomes its longest char-boundary prefix of at
/// most 61 bytes plus `...` (one allocation of at most 64 bytes). Time
/// O(1).
fn error_key_text(key: &Arc<str>) -> Arc<str> {
    if key.len() <= MAX_KEY_LEN {
        return Arc::clone(key);
    }
    let room = MAX_KEY_LEN - CUT_MARK.len();
    // A char is at most 4 bytes, so this looks at no more than 4 positions;
    // position 0 is always a boundary.
    let end = (0..=room)
        .rev()
        .find(|&i| key.is_char_boundary(i))
        .unwrap_or(0);
    let mut text = String::with_capacity(MAX_KEY_LEN);
    text.push_str(key.get(..end).unwrap_or(""));
    text.push_str(CUT_MARK);
    Arc::from(text)
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
    /// use aegis_model::{ElementNamesDef, Methodology, MethodologyDef, MethodologyError};
    ///
    /// let def = MethodologyDef {
    ///     key: "Not-A-Key".into(),
    ///     version: "1".into(),
    ///     label: "Bad".into(),
    ///     element_names: ElementNamesDef::default(),
    ///     impact_categories: vec![],
    ///     feasibility_levels: vec![],
    ///     risk_levels: vec![],
    ///     properties: vec![],
    ///     threat_categories: vec![],
    ///     catalog_schemes: vec![],
    ///     require_rationale: false,
    ///     clause_refs: std::collections::BTreeMap::new(),
    ///     attack_potential: None,
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

        let mut impacts = Self::build_impacts(&def.impact_categories)?;
        let feasibility = Self::build_feasibility_levels(&def.feasibility_levels)?;
        let risks = Self::build_risk_levels(&def.risk_levels)?;
        Self::build_risk_matrices(
            &def.impact_categories,
            &mut impacts.items,
            feasibility.items.len(),
            &risks.by_key,
        )?;
        let properties = Self::build_properties(&def.properties)?;
        let all_properties = (0..properties.items.len())
            .filter_map(|i| u8::try_from(i).ok().map(Property))
            .collect();
        let threats = Self::build_threats(&def.threat_categories, &properties.by_key)?;
        let schemes = Self::build_schemes(&def.catalog_schemes)?;
        let (factors, potential_bands) = match &def.attack_potential {
            Some(ap) => Self::build_attack_potential(ap, &feasibility.by_key)?,
            None => (
                Vocab {
                    items: Vec::new(),
                    by_key: BTreeMap::new(),
                },
                Vec::new(),
            ),
        };
        Self::check_element_names(&def.element_names)?;
        let interface_kinds = Self::build_interface_kinds(&def.element_names.interface_kinds)?;
        let attack_types = Self::build_attack_types(&def.element_names.attack_types)?;
        let control_kinds = Self::build_control_kinds(
            &def.element_names.control_kinds,
            &interface_kinds.by_key,
            &attack_types.by_key,
            &feasibility.by_key,
        )?;
        Self::check_translations(&def.element_names.translations)?;
        Self::check_clause_refs(&def.clause_refs)?;
        let pack_digest = pack_digest_of(&def);
        let translations = def.element_names.translations.clone();

        Ok(Methodology {
            key: def.key,
            version: def.version,
            label: def.label,
            element_names: def.element_names,
            interface_kinds: interface_kinds.items,
            interface_by_key: interface_kinds.by_key,
            attack_types: attack_types.items,
            attack_by_key: attack_types.by_key,
            control_kinds: control_kinds.items,
            control_kind_by_key: control_kinds.by_key,
            translations,
            impact_categories: impacts.items,
            impact_by_key: impacts.by_key,
            feasibility_levels: feasibility.items,
            feasibility_by_key: feasibility.by_key,
            risk_levels: risks.items,
            risk_by_key: risks.by_key,
            properties: properties.items,
            property_by_key: properties.by_key,
            threat_categories: threats.items,
            threat_by_key: threats.by_key,
            schemes: schemes.items,
            scheme_by_key: schemes.by_key,
            factors: factors.items,
            factor_by_key: factors.by_key,
            potential_bands,
            require_rationale: def.require_rationale,
            clause_refs: def.clause_refs,
            all_properties,
            pack_digest,
        })
    }

    /// Immutable content digest (`sha256:<64 lowercase hex>`) of this
    /// methodology's canonical pack JSON (223-methodology-pins).
    ///
    /// Any change to the risk matrix, impact/feasibility levels, attack
    /// potential bands, or vocabularies changes this digest even when
    /// [`key`](Self::key) and [`version`](Self::version) stay the same.
    #[must_use]
    pub fn pack_digest(&self) -> &str {
        &self.pack_digest
    }

    fn check_element_names(names: &ElementNamesDef) -> Result<(), MethodologyError> {
        let pairs: [(&str, &ElementNameDef); 11] = [
            ("component", &names.component),
            ("interface", &names.interface),
            ("data_flow", &names.data_flow),
            ("trust_boundary", &names.trust_boundary),
            ("asset", &names.asset),
            ("damage_scenario", &names.damage_scenario),
            ("threat_scenario", &names.threat_scenario),
            ("attack_tree", &names.attack_tree),
            ("security_control", &names.security_control),
            ("cybersecurity_goal", &names.cybersecurity_goal),
            ("cybersecurity_claim", &names.cybersecurity_claim),
        ];
        for (key, pair) in pairs {
            let k: Arc<str> = Arc::from(key);
            check_label(&k, &pair.singular)?;
            check_label(&k, &pair.plural)?;
        }
        let feas_key: Arc<str> = Arc::from("feasibility");
        check_label(&feas_key, &names.feasibility)?;
        Ok(())
    }

    fn build_interface_kinds(defs: &[LevelDef]) -> Result<Vocab<Entry>, MethodologyError> {
        check_count(defs.len(), 0, MAX_INTERFACE_KINDS, || {
            MethodologyError::TooManyInterfaceKinds {
                max: MAX_INTERFACE_KINDS,
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, l) in defs.iter().enumerate() {
            out.push(add_entry(
                &l.key,
                &l.label,
                pos,
                &mut by_key,
                MethodologyError::DuplicateInterfaceKind,
            )?);
        }
        Ok(Vocab { items: out, by_key })
    }

    fn build_attack_types(defs: &[LevelDef]) -> Result<Vocab<Entry>, MethodologyError> {
        check_count(defs.len(), 0, MAX_ATTACK_TYPES, || {
            MethodologyError::TooManyAttackTypes {
                max: MAX_ATTACK_TYPES,
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, l) in defs.iter().enumerate() {
            out.push(add_entry(
                &l.key,
                &l.label,
                pos,
                &mut by_key,
                MethodologyError::DuplicateAttackType,
            )?);
        }
        Ok(Vocab { items: out, by_key })
    }

    /// Validates the setting presets and resolves their keys. Time
    /// O(K * (I + R + F)), F <= 64 conflicts per preset, with K <= 64
    /// presets, I <= 32 interface kinds and R <= 16 rules each.
    fn build_control_kinds(
        defs: &[ControlKindDef],
        interface_by_key: &BTreeMap<Arc<str>, u8>,
        attack_by_key: &BTreeMap<Arc<str>, u8>,
        feasibility_by_key: &BTreeMap<Arc<str>, u8>,
    ) -> Result<Vocab<ControlKindData>, MethodologyError> {
        check_count(defs.len(), 0, MAX_CONTROL_KINDS, || {
            MethodologyError::TooManyControlKinds {
                max: MAX_CONTROL_KINDS,
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, d) in defs.iter().enumerate() {
            let entry = add_entry(
                &d.key,
                &d.label,
                pos,
                &mut by_key,
                MethodologyError::DuplicateControlKind,
            )?;
            let mut interface_kinds: Vec<InterfaceKind> =
                Vec::with_capacity(d.interface_kinds.len());
            for ik in &d.interface_kinds {
                let Some(&i) = interface_by_key.get(ik) else {
                    return Err(MethodologyError::UnknownControlKindInterfaceKind {
                        control_kind: Arc::clone(&d.key),
                        interface_kind: error_key_text(ik),
                    });
                };
                if interface_kinds.contains(&InterfaceKind(i)) {
                    return Err(MethodologyError::RepeatedControlKindInterfaceKind {
                        control_kind: Arc::clone(&d.key),
                        interface_kind: Arc::clone(ik),
                    });
                }
                interface_kinds.push(InterfaceKind(i));
            }
            interface_kinds.sort_unstable();
            check_count(d.rules.len(), 1, MAX_CONTROL_RULES, || {
                MethodologyError::ControlKindRuleCount {
                    control_kind: Arc::clone(&d.key),
                    max: MAX_CONTROL_RULES,
                }
            })?;
            let mut rules = Vec::with_capacity(d.rules.len());
            for (attack, level) in &d.rules {
                let Some(&a) = attack_by_key.get(attack) else {
                    return Err(MethodologyError::UnknownControlKindAttackType {
                        control_kind: Arc::clone(&d.key),
                        attack_type: error_key_text(attack),
                    });
                };
                let Some(&f) = feasibility_by_key.get(level) else {
                    return Err(MethodologyError::UnknownControlKindFeasibilityLevel {
                        control_kind: Arc::clone(&d.key),
                        level: error_key_text(level),
                    });
                };
                rules.push(ControlRule::new(AttackType(a), FeasibilityLevel(f)));
            }
            rules.sort_unstable();
            out.push(ControlKindData {
                entry,
                interface_kinds,
                rules,
                conflicts_with: Vec::new(),
            });
        }
        // Resolve conflicts_with now that by_key is complete.
        for (pos, d) in defs.iter().enumerate() {
            let mut resolved: Vec<ControlKind> = Vec::with_capacity(d.conflicts_with.len());
            let mut seen: BTreeSet<u8> = BTreeSet::new();
            for other_key in &d.conflicts_with {
                if other_key.as_ref() == d.key.as_ref() {
                    return Err(MethodologyError::ControlKindConflictsWithItself {
                        control_kind: Arc::clone(&d.key),
                    });
                }
                let Some(&other_pos) = by_key.get(other_key) else {
                    return Err(MethodologyError::UnknownControlKindConflict {
                        control_kind: Arc::clone(&d.key),
                        other: error_key_text(other_key),
                    });
                };
                if !seen.insert(other_pos) {
                    return Err(MethodologyError::RepeatedControlKindConflict {
                        control_kind: Arc::clone(&d.key),
                        other: error_key_text(other_key),
                    });
                }
                resolved.push(ControlKind(other_pos));
            }
            resolved.sort_unstable();
            out[pos].conflicts_with = resolved;
        }
        Ok(Vocab { items: out, by_key })
    }

    /// The pack format's caps and key/label rules for `clause_refs`, so a def
    /// built in code cannot produce a pack that `read_pack` then refuses.
    fn check_clause_refs(refs: &BTreeMap<Arc<str>, Vec<Arc<str>>>) -> Result<(), MethodologyError> {
        let too_many = || MethodologyError::TooManyClauseRefs {
            max: MAX_PACK_CLAUSE_REFS,
        };
        check_count(refs.len(), 0, MAX_PACK_CLAUSE_REFS, too_many)?;
        for (page, list) in refs {
            check_key(page)?;
            check_count(list.len(), 0, MAX_PACK_CLAUSE_REFS, too_many)?;
            for r in list {
                check_label(page, r)?;
            }
        }
        Ok(())
    }

    fn check_translations(
        translations: &BTreeMap<Arc<str>, PackTranslationDef>,
    ) -> Result<(), MethodologyError> {
        check_count(translations.len(), 0, MAX_PACK_TRANSLATIONS, || {
            MethodologyError::TooManyTranslations {
                max: MAX_PACK_TRANSLATIONS,
            }
        })?;
        for (lang, tr) in translations {
            check_key(lang)?;
            if let Some(lbl) = &tr.label {
                check_label(lang, lbl)?;
            }
            if let Some(names) = &tr.element_names {
                Self::check_element_names(names)?;
            }
            for maps in [
                &tr.impact_categories,
                &tr.impact_levels,
                &tr.feasibility_levels,
                &tr.risk_levels,
                &tr.properties,
                &tr.threat_categories,
                &tr.catalog_schemes,
                &tr.feasibility_factors,
                &tr.factor_levels,
                &tr.interface_kinds,
                &tr.attack_types,
                &tr.control_kinds,
            ] {
                for (k, v) in maps {
                    check_key(k)?;
                    check_label(k, v)?;
                }
            }
        }
        Ok(())
    }

    fn build_feasibility_levels(defs: &[LevelDef]) -> Result<Vocab<Entry>, MethodologyError> {
        check_count(defs.len(), MIN_LEVELS, MAX_LEVELS, || {
            MethodologyError::FeasibilityLevelCount {
                min: MIN_LEVELS,
                max: MAX_LEVELS,
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, l) in defs.iter().enumerate() {
            out.push(add_entry(
                &l.key,
                &l.label,
                pos,
                &mut by_key,
                MethodologyError::DuplicateFeasibilityLevel,
            )?);
        }
        Ok(Vocab { items: out, by_key })
    }

    fn build_risk_levels(defs: &[LevelDef]) -> Result<Vocab<Entry>, MethodologyError> {
        check_count(defs.len(), MIN_LEVELS, MAX_LEVELS, || {
            MethodologyError::RiskLevelCount {
                min: MIN_LEVELS,
                max: MAX_LEVELS,
            }
        })?;
        let mut by_key = BTreeMap::new();
        let mut out = Vec::with_capacity(defs.len());
        for (pos, l) in defs.iter().enumerate() {
            out.push(add_entry(
                &l.key,
                &l.label,
                pos,
                &mut by_key,
                MethodologyError::DuplicateRiskLevel,
            )?);
        }
        Ok(Vocab { items: out, by_key })
    }

    fn build_risk_matrices(
        defs: &[ImpactCategoryDef],
        categories: &mut [CategoryData],
        feasibility_count: usize,
        risk_by_key: &BTreeMap<Arc<str>, u8>,
    ) -> Result<(), MethodologyError> {
        for (def, cat) in defs.iter().zip(categories.iter_mut()) {
            let expected_rows = cat.levels.len();
            let expected_cols = feasibility_count;
            if def.risk_matrix.len() != expected_rows
                || def.risk_matrix.iter().any(|row| row.len() != expected_cols)
            {
                return Err(MethodologyError::RiskMatrixDimensions {
                    category: Arc::clone(&def.key),
                    expected_rows,
                    expected_cols,
                });
            }
            let mut matrix = Vec::with_capacity(expected_rows * expected_cols);
            for row in &def.risk_matrix {
                for key in row {
                    let Some(&r) = risk_by_key.get(key) else {
                        return Err(MethodologyError::UnknownRiskLevel {
                            category: Arc::clone(&def.key),
                            risk: error_key_text(key),
                        });
                    };
                    matrix.push(RiskLevel(r));
                }
            }
            cat.risk_matrix = matrix.into_boxed_slice();
        }
        Ok(())
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
                risk_matrix: Box::new([]),
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
                        property: error_key_text(key),
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

    fn build_attack_potential(
        ap: &AttackPotentialDef,
        feasibility_by_key: &BTreeMap<Arc<str>, u8>,
    ) -> Result<(Vocab<FactorData>, Vec<PotentialBand>), MethodologyError> {
        check_count(
            ap.factors.len(),
            MIN_FEASIBILITY_FACTORS,
            MAX_FEASIBILITY_FACTORS,
            || MethodologyError::FeasibilityFactorCount {
                min: MIN_FEASIBILITY_FACTORS,
                max: MAX_FEASIBILITY_FACTORS,
            },
        )?;
        let mut factor_by_key = BTreeMap::new();
        let mut factors = Vec::with_capacity(ap.factors.len());
        for (pos, f) in ap.factors.iter().enumerate() {
            let entry = add_entry(
                &f.key,
                &f.label,
                pos,
                &mut factor_by_key,
                MethodologyError::DuplicateFeasibilityFactor,
            )?;
            check_count(f.levels.len(), MIN_LEVELS, MAX_LEVELS, || {
                MethodologyError::FactorLevelCount {
                    factor: Arc::clone(&f.key),
                    min: MIN_LEVELS,
                    max: MAX_LEVELS,
                }
            })?;
            let mut level_by_key = BTreeMap::new();
            let mut levels = Vec::with_capacity(f.levels.len());
            for (lpos, fl) in f.levels.iter().enumerate() {
                let level_entry = add_entry(&fl.key, &fl.label, lpos, &mut level_by_key, |k| {
                    MethodologyError::DuplicateFactorLevel {
                        factor: Arc::clone(&f.key),
                        level: k,
                    }
                })?;
                if fl.points > MAX_FACTOR_POINTS {
                    return Err(MethodologyError::FactorPointsTooLarge {
                        factor: Arc::clone(&f.key),
                        level: Arc::clone(&fl.key),
                        max: MAX_FACTOR_POINTS,
                    });
                }
                levels.push(FactorLevelData {
                    entry: level_entry,
                    points: fl.points,
                });
            }
            factors.push(FactorData {
                entry,
                levels,
                level_by_key,
            });
        }

        check_count(ap.bands.len(), 1, MAX_LEVELS, || {
            MethodologyError::PotentialBandCount {
                min: 1,
                max: MAX_LEVELS,
            }
        })?;
        let mut bands = Vec::with_capacity(ap.bands.len());
        let mut expected_min: u32 = 0;
        for (i, b) in ap.bands.iter().enumerate() {
            if u32::from(b.min) != expected_min {
                return Err(MethodologyError::PotentialBandRange { band: i });
            }
            if i + 1 < ap.bands.len() {
                let Some(m) = b.max else {
                    return Err(MethodologyError::PotentialBandRange { band: i });
                };
                if m < b.min {
                    return Err(MethodologyError::PotentialBandRange { band: i });
                }
                expected_min = u32::from(m) + 1;
            } else if b.max.is_some() {
                return Err(MethodologyError::PotentialBandRange { band: i });
            }
            let Some(&f) = feasibility_by_key.get(&b.feasibility) else {
                return Err(MethodologyError::UnknownBandFeasibilityLevel {
                    band: i,
                    feasibility: error_key_text(&b.feasibility),
                });
            };
            bands.push(PotentialBand {
                min: b.min,
                max: b.max,
                feasibility: FeasibilityLevel(f),
            });
        }

        Ok((
            Vocab {
                items: factors,
                by_key: factor_by_key,
            },
            bands,
        ))
    }

    // -- Identity ----------------------------------------------------------

    /// Returns the stable key. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// assert!(!Methodology::iso21434_r155()?.label().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the pack-defined display names for element types and the
    /// feasibility or likelihood scale (DOM-01, DOM-02). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(&*m.element_names().damage_scenario.singular, "Damage scenario");
    /// assert_eq!(&*m.element_names().feasibility, "Attack feasibility");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn element_names(&self) -> &ElementNamesDef {
        &self.element_names
    }

    /// Returns the display label of the feasibility or likelihood scale, for
    /// example `"Attack feasibility"` or `"Likelihood"` (DOM-02). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// assert_eq!(Methodology::iso21434_r155()?.feasibility_label(), "Attack feasibility");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_label(&self) -> &str {
        &self.element_names.feasibility
    }

    /// Returns `true` if this methodology declares a multi-factor
    /// `attack_potential` scheme, or `false` if steps and threats are rated
    /// directly on the plain [`feasibility_levels`](Self::feasibility_levels)
    /// scale (DOM-02). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// assert!(Methodology::iso21434_r155()?.has_attack_potential());
    /// assert!(!Methodology::eu_cra()?.has_attack_potential());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn has_attack_potential(&self) -> bool {
        !self.factors.is_empty()
    }

    /// Whether the pack asks for a rationale next to each impact and direct
    /// feasibility rating (FND-09); the lint then warns about ratings
    /// without one. Off in the built-in packs. Time O(1).
    #[must_use]
    pub const fn requires_rationale(&self) -> bool {
        self.require_rationale
    }

    /// Returns the ISO/CRA clause references declared for `page`, or an empty
    /// slice when none. Time O(log n) where n is the number of pages with
    /// refs.
    #[must_use]
    pub fn clause_refs(&self, page: &str) -> &[Arc<str>] {
        self.clause_refs.get(page).map_or(&[], Vec::as_slice)
    }

    /// Returns the full clause-reference map (page key -> list of refs).
    #[must_use]
    pub fn clause_refs_map(&self) -> &BTreeMap<Arc<str>, Vec<Arc<str>>> {
        &self.clause_refs
    }

    // -- Impact categories and levels --------------------------------------

    /// Iterates the impact categories in declaration order. Time O(1) per
    /// item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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

    // -- Feasibility levels ----------------------------------------------------

    /// Iterates the feasibility scale from lowest (`0`) to highest feasibility:
    /// `(index, key, label)`. Time O(1) per item, space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let keys: Vec<_> = m.feasibility_levels().map(|(_, k, _)| k).collect();
    /// assert_eq!(keys, ["very-low", "low", "medium", "high"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn feasibility_levels(
        &self,
    ) -> impl ExactSizeIterator<Item = (FeasibilityLevel, &str, &str)> + '_ {
        self.feasibility_levels.iter().enumerate().map(|(i, e)| {
            let idx = u8::try_from(i).unwrap_or(u8::MAX);
            (FeasibilityLevel(idx), &*e.key, &*e.label)
        })
    }

    /// Resolves a feasibility level key in O(log L), L <= 16; space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.feasibility_level("medium").map(|l| l.index()), Some(2));
    /// assert_eq!(m.feasibility_level("nope"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_level(&self, key: &str) -> Option<FeasibilityLevel> {
        self.feasibility_by_key
            .get(key)
            .copied()
            .map(FeasibilityLevel)
    }

    /// Returns the key of `level` in O(1), or `None` if `level` is outside this
    /// methodology's feasibility scale. Space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let high = m.feasibility_level("high");
    /// assert_eq!(high.and_then(|l| m.feasibility_level_key(l)), Some("high"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_level_key(&self, level: FeasibilityLevel) -> Option<&str> {
        self.feasibility_levels.get(level.index()).map(|e| &*e.key)
    }

    /// Returns the human-readable label of `level` in O(1), or `None` if
    /// `level` is outside this methodology's feasibility scale. Space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let low = m.feasibility_level("very-low");
    /// assert_eq!(low.and_then(|l| m.feasibility_level_label(l)), Some("Very low"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_level_label(&self, level: FeasibilityLevel) -> Option<&str> {
        self.feasibility_levels
            .get(level.index())
            .map(|e| &*e.label)
    }

    // -- Risk levels and risk matrices ----------------------------------------

    /// Iterates the risk scale in declaration order, lowest risk first:
    /// `(index, key, label)`. Time O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let keys: Vec<_> = m.risk_levels().map(|(_, k, _)| k).collect();
    /// assert_eq!(keys, ["r1", "r2", "r3", "r4", "r5"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn risk_levels(&self) -> impl ExactSizeIterator<Item = (RiskLevel, &str, &str)> + '_ {
        self.risk_levels.iter().enumerate().map(|(i, e)| {
            let idx = u8::try_from(i).unwrap_or(u8::MAX);
            (RiskLevel(idx), &*e.key, &*e.label)
        })
    }

    /// Returns the number of risk levels in this methodology's scale. Time
    /// O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// assert_eq!(Methodology::iso21434_r155()?.risk_level_count(), 5);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn risk_level_count(&self) -> usize {
        self.risk_levels.len()
    }

    /// Resolves a risk level key in O(log R), R <= 16; space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.risk_level("r5").map(|r| r.index()), Some(4));
    /// assert_eq!(m.risk_level("nope"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn risk_level(&self, key: &str) -> Option<RiskLevel> {
        self.risk_by_key.get(key).copied().map(RiskLevel)
    }

    /// Returns the key of `level` in O(1), or `None` if `level` is outside
    /// this methodology's risk scale. Space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let r4 = m.risk_level("r4");
    /// assert_eq!(r4.and_then(|r| m.risk_level_key(r)), Some("r4"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn risk_level_key(&self, level: RiskLevel) -> Option<&str> {
        self.risk_levels.get(level.index()).map(|e| &*e.key)
    }

    /// Returns the human-readable label of `level` in O(1), or `None` if
    /// `level` is outside this methodology's risk scale. Space O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let r1 = m.risk_level("r1");
    /// assert_eq!(r1.and_then(|r| m.risk_level_label(r)), Some("Risk 1"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn risk_level_label(&self, level: RiskLevel) -> Option<&str> {
        self.risk_levels.get(level.index()).map(|e| &*e.label)
    }

    /// Looks up the risk level for `(category, impact, feasibility)` in
    /// `category`'s 2D risk matrix in O(1) time and O(1) space. Returns `None`
    /// if `category`, `impact`, or `feasibility` is out of range for this
    /// methodology.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let safety = m.impact_category("safety").ok_or("no category")?;
    /// let severe = m.level(safety, "severe").ok_or("no level")?;
    /// let high = m.feasibility_level("high").ok_or("no feasibility")?;
    /// let r5 = m.risk_level("r5").ok_or("no risk")?;
    /// assert_eq!(m.risk_for(safety, severe, high), Some(r5));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn risk_for(
        &self,
        category: ImpactCategory,
        impact: Level,
        feasibility: FeasibilityLevel,
    ) -> Option<RiskLevel> {
        let cat = self.impact_categories.get(category.index())?;
        let cols = self.feasibility_levels.len();
        if impact.index() >= cat.levels.len() || feasibility.index() >= cols {
            return None;
        }
        cat.risk_matrix
            .get(impact.index() * cols + feasibility.index())
            .copied()
    }

    // -- Properties ----------------------------------------------------------

    /// Iterates the security properties in declaration order. Time O(1) per
    /// item.
    ///
    /// ```
    /// use aegis_model::{Methodology, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// let syntax = m.catalog_syntax(scheme).ok_or("no syntax")?.clone();
    /// let r = m.parse_catalog_ref(scheme, "4.1")?;
    /// assert_eq!(r.segments(), [4, 1]);
    /// assert_eq!(
    ///     m.parse_catalog_ref(scheme, "4.0"),
    ///     Err(ModelError::InvalidCatalogId { scheme, syntax }),
    /// );
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

    // -- Attack potential factors and bands ------------------------------------

    /// Iterates the attack potential factors in declaration order. Empty if the
    /// methodology declares no `attack_potential` scheme. Time O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let keys: Vec<_> = m
    ///     .feasibility_factors()
    ///     .filter_map(|f| m.feasibility_factor_key(f))
    ///     .collect();
    /// assert_eq!(
    ///     keys,
    ///     [
    ///         "elapsed-time",
    ///         "expertise",
    ///         "knowledge",
    ///         "window-of-opportunity",
    ///         "equipment",
    ///     ]
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn feasibility_factors(&self) -> impl ExactSizeIterator<Item = FeasibilityFactor> + use<> {
        index_range(self.factors.len()).map(FeasibilityFactor)
    }

    /// Returns the number of attack potential factors (0 if the methodology
    /// declares no `attack_potential` scheme). Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// assert_eq!(Methodology::iso21434_r155()?.feasibility_factor_count(), 5);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_factor_count(&self) -> usize {
        self.factors.len()
    }

    /// Resolves an attack potential factor key in O(log F), F <= 16.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.feasibility_factor("equipment").map(|f| f.index()), Some(4));
    /// assert_eq!(m.feasibility_factor("nope"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_factor(&self, key: &str) -> Option<FeasibilityFactor> {
        self.factor_by_key.get(key).copied().map(FeasibilityFactor)
    }

    /// Returns an attack potential factor's key, or `None` if `factor` is
    /// outside this methodology. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let f = m.feasibility_factor("knowledge");
    /// assert_eq!(f.and_then(|f| m.feasibility_factor_key(f)), Some("knowledge"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_factor_key(&self, factor: FeasibilityFactor) -> Option<&str> {
        self.factor_data(factor).map(|f| &*f.entry.key)
    }

    /// Returns an attack potential factor's display label, or `None` if
    /// `factor` is outside this methodology. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let f = m.feasibility_factor("knowledge");
    /// assert_eq!(
    ///     f.and_then(|f| m.feasibility_factor_label(f)),
    ///     Some("Knowledge of the item")
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_factor_label(&self, factor: FeasibilityFactor) -> Option<&str> {
        self.factor_data(factor).map(|f| &*f.entry.label)
    }

    /// Iterates the levels of one attack potential factor's scale in
    /// declaration order. Empty if `factor` is not in this methodology. Time
    /// O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let exp = m.feasibility_factor("expertise").ok_or("no factor")?;
    /// let keys: Vec<_> = m
    ///     .factor_levels(exp)
    ///     .filter_map(|l| m.factor_level_key(exp, l))
    ///     .collect();
    /// assert_eq!(keys, ["layman", "proficient", "expert", "multiple-experts"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn factor_levels(
        &self,
        factor: FeasibilityFactor,
    ) -> impl ExactSizeIterator<Item = FactorLevel> + use<> {
        index_range(self.factor_level_count(factor)).map(FactorLevel)
    }

    /// Returns the number of levels in `factor`'s scale, or 0 if `factor` is
    /// not in this methodology. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let et = m.feasibility_factor("elapsed-time").ok_or("no factor")?;
    /// assert_eq!(m.factor_level_count(et), 5);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn factor_level_count(&self, factor: FeasibilityFactor) -> usize {
        self.factor_data(factor).map_or(0, |f| f.levels.len())
    }

    /// Resolves a level key within one attack potential factor's scale in
    /// O(log L), L <= 16.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let eq = m.feasibility_factor("equipment").ok_or("no factor")?;
    /// assert_eq!(m.factor_level(eq, "bespoke").map(|l| l.index()), Some(2));
    /// assert_eq!(m.factor_level(eq, "nope"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn factor_level(&self, factor: FeasibilityFactor, key: &str) -> Option<FactorLevel> {
        self.factor_data(factor)
            .and_then(|f| f.level_by_key.get(key).copied())
            .map(FactorLevel)
    }

    /// Returns a factor level's key, or `None` if `factor` or `level` is out of
    /// range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let eq = m.feasibility_factor("equipment").ok_or("no factor")?;
    /// let top = m.factor_levels(eq).last().ok_or("no level")?;
    /// assert_eq!(m.factor_level_key(eq, top), Some("multiple-bespoke"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn factor_level_key(&self, factor: FeasibilityFactor, level: FactorLevel) -> Option<&str> {
        self.factor_level_data(factor, level).map(|d| &*d.entry.key)
    }

    /// Returns a factor level's display label, or `None` if `factor` or `level`
    /// is out of range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let eq = m.feasibility_factor("equipment").ok_or("no factor")?;
    /// let first = m.factor_levels(eq).next().ok_or("no level")?;
    /// assert_eq!(m.factor_level_label(eq, first), Some("Standard"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn factor_level_label(
        &self,
        factor: FeasibilityFactor,
        level: FactorLevel,
    ) -> Option<&str> {
        self.factor_level_data(factor, level)
            .map(|d| &*d.entry.label)
    }

    /// Returns the point contribution of `(factor, level)`, or `None` if out of
    /// range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let eq = m.feasibility_factor("equipment").ok_or("no factor")?;
    /// let bespoke = m.factor_level(eq, "bespoke").ok_or("no level")?;
    /// assert_eq!(m.factor_level_points(eq, bespoke), Some(7));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn factor_level_points(
        &self,
        factor: FeasibilityFactor,
        level: FactorLevel,
    ) -> Option<u16> {
        self.factor_level_data(factor, level).map(|d| d.points)
    }

    /// Returns the score bands of this methodology's attack potential scheme
    /// (empty if the methodology declares no `attack_potential` scheme). Time
    /// O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.potential_bands().len(), 4);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn potential_bands(&self) -> &[PotentialBand] {
        &self.potential_bands
    }

    /// Looks up the [`PotentialBand`] that contains `score`, or `None` if this
    /// methodology declares no `attack_potential` scheme. Time O(B), B <= 16.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let b = m.potential_band_for_score(22).ok_or("no band")?;
    /// assert_eq!(m.feasibility_level_key(b.feasibility()), Some("low"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn potential_band_for_score(&self, score: u32) -> Option<PotentialBand> {
        self.potential_bands
            .iter()
            .copied()
            .find(|b| b.contains(score))
    }

    /// Evaluates a complete per-factor rating slice into an [`AttackPotential`]
    /// breakdown. Returns `None` if this methodology declares no
    /// `attack_potential` scheme, if `ratings.len() != self.factors.len()`, or
    /// if any factor in `ratings` is `None` or out of range. Time O(F), F <= 16.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let ratings: Vec<_> = m
    ///     .feasibility_factors()
    ///     .map(|f| m.factor_levels(f).next())
    ///     .collect();
    /// let ap = m.resolve_attack_potential(&ratings).ok_or("incomplete")?;
    /// assert_eq!(ap.score(), 0);
    /// assert_eq!(m.feasibility_level_key(ap.feasibility()), Some("high"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn resolve_attack_potential(
        &self,
        ratings: &[Option<FactorLevel>],
    ) -> Option<AttackPotential> {
        if self.factors.is_empty() || ratings.len() != self.factors.len() {
            return None;
        }
        let mut factors = Vec::with_capacity(self.factors.len());
        let mut score: u32 = 0;
        for (i, (&opt_level, factor_data)) in ratings.iter().zip(&self.factors).enumerate() {
            let level = opt_level?;
            let level_data = factor_data.levels.get(level.index())?;
            let factor = FeasibilityFactor(u8::try_from(i).ok()?);
            score += u32::from(level_data.points);
            factors.push(FactorContribution {
                factor,
                level,
                points: level_data.points,
            });
        }
        let band = self.potential_band_for_score(score)?;
        Some(AttackPotential {
            factors: factors.into_boxed_slice(),
            score,
            band,
            feasibility: band.feasibility(),
        })
    }

    /// Evaluates a complete per-factor rating slice directly to its derived
    /// [`FeasibilityLevel`] without allocating the factor breakdown. Returns
    /// `None` under the same conditions as [`Self::resolve_attack_potential`].
    /// Time O(F), F <= 16; space O(1).
    pub(crate) fn resolve_attack_potential_feasibility(
        &self,
        ratings: &[Option<FactorLevel>],
    ) -> Option<FeasibilityLevel> {
        if self.factors.is_empty() || ratings.len() != self.factors.len() {
            return None;
        }
        let mut score: u32 = 0;
        for (&opt_level, factor_data) in ratings.iter().zip(&self.factors) {
            let level = opt_level?;
            let level_data = factor_data.levels.get(level.index())?;
            score += u32::from(level_data.points);
        }
        self.potential_band_for_score(score)
            .map(PotentialBand::feasibility)
    }

    // -- Interface kinds (ARC-02, Decision D15) -------------------------------

    /// Iterates the item definition interface kinds in declaration order. Time
    /// O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.interface_kinds().count() >= 4);
    /// # Ok(())
    /// # }
    /// ```
    pub fn interface_kinds(&self) -> impl ExactSizeIterator<Item = InterfaceKind> + use<> {
        index_range(self.interface_kinds.len()).map(InterfaceKind)
    }

    /// Returns the number of declared interface kinds. Time O(1).
    #[must_use]
    pub fn interface_kind_count(&self) -> usize {
        self.interface_kinds.len()
    }

    /// Resolves an interface kind key in O(log K), K <= 32.
    #[must_use]
    pub fn interface_kind(&self, key: &str) -> Option<InterfaceKind> {
        self.interface_by_key.get(key).copied().map(InterfaceKind)
    }

    /// Returns an interface kind's key, or `None` if out of range. Time O(1).
    #[must_use]
    pub fn interface_kind_key(&self, kind: InterfaceKind) -> Option<&str> {
        self.interface_kinds.get(kind.index()).map(|e| &*e.key)
    }

    /// Returns an interface kind's display label, or `None` if out of range. Time O(1).
    #[must_use]
    pub fn interface_kind_label(&self, kind: InterfaceKind) -> Option<&str> {
        self.interface_kinds.get(kind.index()).map(|e| &*e.label)
    }

    // -- Attack types (CFG-02, Decision D26) ----------------------------------

    /// Iterates the attack types in declaration order. Empty for a pack that
    /// declares none. Time O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.attack_types().count() >= 10);
    /// # Ok(())
    /// # }
    /// ```
    pub fn attack_types(&self) -> impl ExactSizeIterator<Item = AttackType> + use<> {
        index_range(self.attack_types.len()).map(AttackType)
    }

    /// Returns the number of declared attack types. Time O(1).
    #[must_use]
    pub fn attack_type_count(&self) -> usize {
        self.attack_types.len()
    }

    /// Resolves an attack type key in O(log K), K <= 32.
    #[must_use]
    pub fn attack_type(&self, key: &str) -> Option<AttackType> {
        self.attack_by_key.get(key).copied().map(AttackType)
    }

    /// Returns an attack type's key, or `None` if out of range. Time O(1).
    #[must_use]
    pub fn attack_type_key(&self, kind: AttackType) -> Option<&str> {
        self.attack_types.get(kind.index()).map(|e| &*e.key)
    }

    /// Returns an attack type's display label, or `None` if out of range. Time O(1).
    #[must_use]
    pub fn attack_type_label(&self, kind: AttackType) -> Option<&str> {
        self.attack_types.get(kind.index()).map(|e| &*e.label)
    }

    // -- Control kinds: setting presets (CFG-06) ------------------------------

    /// Iterates the setting presets in declaration order. Time O(1) per item.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// for k in m.control_kinds() {
    ///     assert!(m.control_kind_key(k).is_some());
    ///     assert!(!m.control_kind_rules(k).is_empty());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn control_kinds(&self) -> impl ExactSizeIterator<Item = ControlKind> + use<> {
        index_range(self.control_kinds.len()).map(ControlKind)
    }

    /// Resolves a setting preset key in O(log K), K <= 64.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.control_kind("no-such-preset"), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind(&self, key: &str) -> Option<ControlKind> {
        self.control_kind_by_key.get(key).copied().map(ControlKind)
    }

    /// Returns a setting preset's key, or `None` if out of range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.control_kinds().all(|k| m.control_kind_key(k).is_some()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_key(&self, kind: ControlKind) -> Option<&str> {
        self.control_kinds.get(kind.index()).map(|d| &*d.entry.key)
    }

    /// Returns a setting preset's display label, or `None` if out of range.
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.control_kinds().all(|k| m.control_kind_label(k).is_some()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_label(&self, kind: ControlKind) -> Option<&str> {
        self.control_kinds
            .get(kind.index())
            .map(|d| &*d.entry.label)
    }

    /// Returns a setting preset's localized label for `lang`, falling back to
    /// [`Self::control_kind_label`]. Time O(log T + log K).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// for k in m.control_kinds() {
    ///     assert_eq!(m.control_kind_label_for_lang(k, "xx"), m.control_kind_label(k));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_label_for_lang(&self, kind: ControlKind, lang: &str) -> Option<&str> {
        let data = self.control_kinds.get(kind.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.control_kinds.get(&data.entry.key))
        {
            return Some(lbl);
        }
        Some(&data.entry.label)
    }

    /// Returns a setting preset's default rules, sorted by attack type. Empty
    /// when out of range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.control_kinds().all(|k| !m.control_kind_rules(k).is_empty()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_rules(&self, kind: ControlKind) -> &[ControlRule] {
        self.control_kinds
            .get(kind.index())
            .map_or(&[], |d| d.rules.as_slice())
    }

    /// Returns the interface kinds a setting preset is for, sorted by index.
    /// Empty means the preset is for components; also empty when out of
    /// range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let n = m.interface_kinds().count();
    /// assert!(m.control_kinds().all(|k| m.control_kind_interface_kinds(k).len() <= n));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_interface_kinds(&self, kind: ControlKind) -> &[InterfaceKind] {
        self.control_kinds
            .get(kind.index())
            .map_or(&[], |d| d.interface_kinds.as_slice())
    }

    /// Returns the presets this one conflicts with, sorted by index. Empty
    /// when out of range. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert!(m.control_kinds().all(|k| m.control_kind_conflicts(k).is_empty()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_conflicts(&self, kind: ControlKind) -> &[ControlKind] {
        self.control_kinds
            .get(kind.index())
            .map_or(&[], |d| d.conflicts_with.as_slice())
    }

    // -- Translations (L10N-07) -----------------------------------------------

    /// Returns all per-language translation tables declared by this pack. Time O(1).
    #[must_use]
    pub fn translations(&self) -> &BTreeMap<Arc<str>, PackTranslationDef> {
        &self.translations
    }

    /// Returns the translation table for `lang` (for example `"ja"`), if declared.
    #[must_use]
    pub fn translation(&self, lang: &str) -> Option<&PackTranslationDef> {
        self.translations.get(lang)
    }

    /// Returns the display label of the methodology for `lang`, falling back to
    /// [`Self::label`].
    #[must_use]
    pub fn label_for_lang(&self, lang: &str) -> &str {
        self.translation(lang)
            .and_then(|tr| tr.label.as_deref())
            .unwrap_or(&self.label)
    }

    /// Returns the pack-defined element display names for `lang`, falling back
    /// to [`Self::element_names`].
    #[must_use]
    pub fn element_names_for_lang(&self, lang: &str) -> &ElementNamesDef {
        self.translation(lang)
            .and_then(|tr| tr.element_names.as_ref())
            .unwrap_or(&self.element_names)
    }

    /// Returns the display label of the feasibility or likelihood scale for
    /// `lang`, falling back to [`Self::feasibility_label`].
    #[must_use]
    pub fn feasibility_label_for_lang(&self, lang: &str) -> &str {
        &self.element_names_for_lang(lang).feasibility
    }

    /// Returns an impact category's localized label for `lang`, falling back to
    /// [`Self::impact_category_label`].
    #[must_use]
    pub fn impact_category_label_for_lang(&self, cat: ImpactCategory, lang: &str) -> Option<&str> {
        let data = self.category_data(cat)?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.impact_categories.get(&data.entry.key))
        {
            return Some(lbl);
        }
        Some(&data.entry.label)
    }

    /// Returns an impact level's localized label for `lang`, falling back to
    /// [`Self::level_label`].
    #[must_use]
    pub fn level_label_for_lang(
        &self,
        cat: ImpactCategory,
        level: Level,
        lang: &str,
    ) -> Option<&str> {
        let entry = self.level_entry(cat, level)?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.impact_levels.get(&entry.key))
        {
            return Some(lbl);
        }
        Some(&entry.label)
    }

    /// Returns a feasibility level's localized label for `lang`, falling back
    /// to [`Self::feasibility_level_label`].
    #[must_use]
    pub fn feasibility_level_label_for_lang(
        &self,
        level: FeasibilityLevel,
        lang: &str,
    ) -> Option<&str> {
        let entry = self.feasibility_levels.get(level.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.feasibility_levels.get(&entry.key))
        {
            return Some(lbl);
        }
        Some(&entry.label)
    }

    /// Returns a risk level's localized label for `lang`, falling back to
    /// [`Self::risk_level_label`].
    #[must_use]
    pub fn risk_level_label_for_lang(&self, level: RiskLevel, lang: &str) -> Option<&str> {
        let entry = self.risk_levels.get(level.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.risk_levels.get(&entry.key))
        {
            return Some(lbl);
        }
        Some(&entry.label)
    }

    /// Returns a security property's localized label for `lang`, falling back
    /// to [`Self::property_label`].
    #[must_use]
    pub fn property_label_for_lang(&self, p: Property, lang: &str) -> Option<&str> {
        let entry = self.properties.get(p.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.properties.get(&entry.key))
        {
            return Some(lbl);
        }
        Some(&entry.label)
    }

    /// Returns a threat category's localized label for `lang`, falling back to
    /// [`Self::threat_category_label`].
    #[must_use]
    pub fn threat_category_label_for_lang(&self, t: ThreatCategory, lang: &str) -> Option<&str> {
        let data = self.threat_categories.get(t.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.threat_categories.get(&data.entry.key))
        {
            return Some(lbl);
        }
        Some(&data.entry.label)
    }

    /// Returns a catalog scheme's localized label for `lang`, falling back to
    /// [`Self::catalog_scheme_label`].
    #[must_use]
    pub fn catalog_scheme_label_for_lang(&self, s: CatalogScheme, lang: &str) -> Option<&str> {
        let data = self.schemes.get(s.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.catalog_schemes.get(&data.entry.key))
        {
            return Some(lbl);
        }
        Some(&data.entry.label)
    }

    /// Returns an attack potential factor's localized label for `lang`, falling
    /// back to [`Self::feasibility_factor_label`].
    #[must_use]
    pub fn feasibility_factor_label_for_lang(
        &self,
        factor: FeasibilityFactor,
        lang: &str,
    ) -> Option<&str> {
        let data = self.factor_data(factor)?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.feasibility_factors.get(&data.entry.key))
        {
            return Some(lbl);
        }
        Some(&data.entry.label)
    }

    /// Returns an attack potential factor level's localized label for `lang`,
    /// falling back to [`Self::factor_level_label`].
    #[must_use]
    pub fn factor_level_label_for_lang(
        &self,
        factor: FeasibilityFactor,
        level: FactorLevel,
        lang: &str,
    ) -> Option<&str> {
        let data = self.factor_level_data(factor, level)?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.factor_levels.get(&data.entry.key))
        {
            return Some(lbl);
        }
        Some(&data.entry.label)
    }

    /// Returns an interface kind's localized label for `lang`, falling back to
    /// [`Self::interface_kind_label`].
    #[must_use]
    pub fn interface_kind_label_for_lang(&self, kind: InterfaceKind, lang: &str) -> Option<&str> {
        let entry = self.interface_kinds.get(kind.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.interface_kinds.get(&entry.key))
        {
            return Some(lbl);
        }
        Some(&entry.label)
    }

    /// Returns an attack type's localized label for `lang`, falling back to
    /// [`Self::attack_type_label`].
    #[must_use]
    pub fn attack_type_label_for_lang(&self, kind: AttackType, lang: &str) -> Option<&str> {
        let entry = self.attack_types.get(kind.index())?;
        if let Some(lbl) = self
            .translation(lang)
            .and_then(|tr| tr.attack_types.get(&entry.key))
        {
            return Some(lbl);
        }
        Some(&entry.label)
    }

    /// Checks that an attack type is declared in this methodology. Time O(1).
    pub(crate) fn check_attack_type(&self, kind: AttackType) -> Result<(), ModelError> {
        if kind.index() < self.attack_types.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownAttackType(kind))
        }
    }

    /// Checks that a control kind is declared in this methodology. Time O(1).
    pub(crate) fn check_control_kind(&self, kind: ControlKind) -> Result<(), ModelError> {
        if kind.index() < self.control_kinds.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownControlKind(kind))
        }
    }

    /// Checks that an interface kind is declared in this methodology. Time O(1).
    pub(crate) fn check_interface_kind(&self, kind: InterfaceKind) -> Result<(), ModelError> {
        if kind.index() < self.interface_kinds.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownInterfaceKind(kind))
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
            Err(ModelError::InvalidCatalogId {
                scheme: r.scheme(),
                syntax: syntax.clone(),
            })
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

    /// Checks that a feasibility level is in this methodology's scale. Time
    /// O(1).
    pub(crate) fn check_feasibility_level(
        &self,
        level: FeasibilityLevel,
    ) -> Result<(), ModelError> {
        if level.index() < self.feasibility_levels.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownFeasibilityLevel(level))
        }
    }

    /// Checks that a risk level is in this methodology's scale. Time O(1).
    #[allow(dead_code)]
    pub(crate) fn check_risk_level(&self, level: RiskLevel) -> Result<(), ModelError> {
        if level.index() < self.risk_levels.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownRiskLevel(level))
        }
    }

    /// Checks that an attack potential factor is declared in this methodology.
    /// Time O(1).
    pub(crate) fn check_feasibility_factor(
        &self,
        factor: FeasibilityFactor,
    ) -> Result<(), ModelError> {
        if factor.index() < self.factors.len() {
            Ok(())
        } else {
            Err(ModelError::UnknownFeasibilityFactor(factor))
        }
    }

    /// Checks that a factor level is in a declared factor's scale. Time O(1).
    pub(crate) fn check_factor_level(
        &self,
        factor: FeasibilityFactor,
        level: FactorLevel,
    ) -> Result<(), ModelError> {
        self.check_feasibility_factor(factor)?;
        if level.index() < self.factor_level_count(factor) {
            Ok(())
        } else {
            Err(ModelError::UnknownFactorLevel { factor, level })
        }
    }

    /// Checks that every excluded factor and level in `filter` belongs to
    /// this methodology. Time O(F * L).
    pub(crate) fn check_attack_filter(
        &self,
        filter: &crate::applicability::AttackFilter,
    ) -> Result<(), ModelError> {
        for fi in 0..MAX_FEASIBILITY_FACTORS {
            let mask = filter.excluded_mask(fi);
            if mask == 0 {
                continue;
            }
            let Ok(fi_u8) = u8::try_from(fi) else {
                continue;
            };
            let factor = FeasibilityFactor(fi_u8);
            self.check_feasibility_factor(factor)?;
            for li in 0..MAX_LEVELS {
                let bit = u32::try_from(li)
                    .ok()
                    .and_then(|s| 1u16.checked_shl(s))
                    .unwrap_or(0);
                if (mask & bit) != 0 {
                    let Ok(li_u8) = u8::try_from(li) else {
                        continue;
                    };
                    self.check_factor_level(factor, FactorLevel(li_u8))?;
                }
            }
        }
        Ok(())
    }

    fn category_data(&self, cat: ImpactCategory) -> Option<&CategoryData> {
        self.impact_categories.get(cat.index())
    }

    fn level_entry(&self, cat: ImpactCategory, level: Level) -> Option<&Entry> {
        self.category_data(cat)
            .and_then(|c| c.levels.get(level.index()))
    }

    fn factor_data(&self, factor: FeasibilityFactor) -> Option<&FactorData> {
        self.factors.get(factor.index())
    }

    fn factor_level_data(
        &self,
        factor: FeasibilityFactor,
        level: FactorLevel,
    ) -> Option<&FactorLevelData> {
        self.factor_data(factor)
            .and_then(|f| f.levels.get(level.index()))
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
            risk_matrix: (0..levels)
                .map(|r| {
                    (0..3)
                        .map(|c| {
                            if r == 0 || c == 0 {
                                "r0".into()
                            } else {
                                "r1".into()
                            }
                        })
                        .collect()
                })
                .collect(),
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

    fn factor_level_def(key: &str, points: u16) -> FactorLevelDef {
        FactorLevelDef {
            key: key.into(),
            label: key.into(),
            points,
        }
    }

    fn factor_def(key: &str, levels: &[(&str, u16)]) -> FactorDef {
        FactorDef {
            key: key.into(),
            label: key.into(),
            levels: levels
                .iter()
                .map(|&(k, p)| factor_level_def(k, p))
                .collect(),
        }
    }

    fn band_def(min: u16, max: Option<u16>, feasibility: &str) -> PotentialBandDef {
        PotentialBandDef {
            min,
            max,
            feasibility: feasibility.into(),
        }
    }

    fn sample_attack_potential() -> AttackPotentialDef {
        AttackPotentialDef {
            factors: vec![
                factor_def("time", &[("fast", 0), ("slow", 5)]),
                factor_def("skill", &[("low", 0), ("high", 7)]),
            ],
            bands: vec![
                band_def(0, Some(4), "f2"),
                band_def(5, Some(10), "f1"),
                band_def(11, None, "f0"),
            ],
        }
    }

    /// A small valid definition that each test breaks in one way.
    fn base() -> MethodologyDef {
        MethodologyDef {
            key: "base".into(),
            version: "1.0".into(),
            label: "Base".into(),
            element_names: ElementNamesDef::default(),
            impact_categories: vec![category("harm", 3), category("cost", 2)],
            feasibility_levels: vec![level("f0"), level("f1"), level("f2")],
            risk_levels: vec![level("r0"), level("r1")],
            require_rationale: false,
            clause_refs: std::collections::BTreeMap::default(),
            attack_potential: None,
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
        assert_eq!(m.feasibility_levels().len(), 3);
        assert_eq!(m.risk_level_count(), 2);
        assert_eq!(m.risk_levels().len(), 2);
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
                for (f, _, _) in m.feasibility_levels() {
                    assert!(m.risk_for(c, l, f).is_some());
                }
            }
        }
        for (f, key, label) in m.feasibility_levels() {
            assert_eq!(m.feasibility_level(key), Some(f));
            assert_eq!(m.feasibility_level_key(f), Some(key));
            assert_eq!(m.feasibility_level_label(f), Some(label));
        }
        for (r, key, label) in m.risk_levels() {
            assert_eq!(m.risk_level(key), Some(r));
            assert_eq!(m.risk_level_key(r), Some(key));
            assert_eq!(m.risk_level_label(r), Some(label));
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
        assert_eq!(m.feasibility_level_key(FeasibilityLevel(3)), None);
        assert_eq!(m.feasibility_level_label(FeasibilityLevel(3)), None);
        assert_eq!(m.risk_level_key(RiskLevel(2)), None);
        assert_eq!(m.risk_level_label(RiskLevel(2)), None);
        assert_eq!(
            m.risk_for(ImpactCategory(9), Level(0), FeasibilityLevel(0)),
            None
        );
        assert_eq!(
            m.risk_for(ImpactCategory(0), Level(9), FeasibilityLevel(0)),
            None
        );
        assert_eq!(
            m.risk_for(ImpactCategory(0), Level(0), FeasibilityLevel(9)),
            None
        );
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
        assert_eq!(m.check_feasibility_level(FeasibilityLevel(2)), Ok(()));
        assert_eq!(
            m.check_feasibility_level(FeasibilityLevel(3)),
            Err(ModelError::UnknownFeasibilityLevel(FeasibilityLevel(3)))
        );
        assert_eq!(m.check_risk_level(RiskLevel(1)), Ok(()));
        assert_eq!(
            m.check_risk_level(RiskLevel(2)),
            Err(ModelError::UnknownRiskLevel(RiskLevel(2)))
        );
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
    fn methodology_rejects_duplicate_feasibility_level() {
        let mut d = base();
        d.feasibility_levels.push(level("f0"));
        rejects(d, &MethodologyError::DuplicateFeasibilityLevel("f0".into()));
    }

    #[test]
    fn methodology_rejects_feasibility_level_counts() {
        for len in [0, 1, 17] {
            let mut d = base();
            d.feasibility_levels = (0..len).map(|i| level(&format!("f{i}"))).collect();
            rejects(
                d,
                &MethodologyError::FeasibilityLevelCount { min: 2, max: 16 },
            );
        }
        for len in [2, 16] {
            let mut d = base();
            d.feasibility_levels = (0..len).map(|i| level(&format!("f{i}"))).collect();
            for cat in &mut d.impact_categories {
                cat.risk_matrix = vec![vec!["r0".into(); len]; cat.levels.len()];
            }
            assert!(Methodology::new(d).is_ok());
        }
    }

    #[test]
    fn methodology_rejects_duplicate_risk_level() {
        let mut d = base();
        d.risk_levels.push(level("r0"));
        rejects(d, &MethodologyError::DuplicateRiskLevel("r0".into()));
    }

    #[test]
    fn methodology_rejects_risk_level_counts() {
        for len in [0, 1, 17] {
            let mut d = base();
            d.risk_levels = (0..len).map(|i| level(&format!("r{i}"))).collect();
            rejects(d, &MethodologyError::RiskLevelCount { min: 2, max: 16 });
        }
        for len in [2, 16] {
            let mut d = base();
            d.risk_levels = (0..len).map(|i| level(&format!("r{i}"))).collect();
            assert!(Methodology::new(d).is_ok());
        }
    }

    #[test]
    fn methodology_rejects_bad_risk_matrix_dimensions() {
        let mut d = base();
        d.impact_categories[0].risk_matrix.pop();
        rejects(
            d,
            &MethodologyError::RiskMatrixDimensions {
                category: "harm".into(),
                expected_rows: 3,
                expected_cols: 3,
            },
        );
        let mut d = base();
        d.impact_categories[1].risk_matrix[0].pop();
        rejects(
            d,
            &MethodologyError::RiskMatrixDimensions {
                category: "cost".into(),
                expected_rows: 2,
                expected_cols: 3,
            },
        );
    }

    #[test]
    fn methodology_rejects_unknown_risk_level_in_matrix() {
        let mut d = base();
        d.impact_categories[0].risk_matrix[1][2] = "r9".into();
        rejects(
            d,
            &MethodologyError::UnknownRiskLevel {
                category: "harm".into(),
                risk: "r9".into(),
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
        // Over 64 bytes, the error keeps a cut text (job 012).
        let cut = format!("a{}...", "b".repeat(60));
        rejects(d, &MethodologyError::InvalidKey(cut.as_str().into()));
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

    /// The key text an error from `new` keeps, for the variants that can
    /// hold an unvalidated key.
    fn error_key(e: &MethodologyError) -> Option<&Arc<str>> {
        match e {
            MethodologyError::InvalidKey(k) => Some(k),
            MethodologyError::UnknownRiskLevel { risk, .. } => Some(risk),
            MethodologyError::UnknownViolatedProperty { property, .. } => Some(property),
            MethodologyError::UnknownBandFeasibilityLevel { feasibility, .. } => Some(feasibility),
            _ => None,
        }
    }

    /// Runs `new` on `d` and returns the error with its key text.
    fn key_error(d: MethodologyDef) -> (MethodologyError, Arc<str>) {
        let e = Methodology::new(d).expect_err("an error");
        let k = Arc::clone(error_key(&e).expect("a key-text error"));
        (e, k)
    }

    #[test]
    fn methodology_error_key_text_is_bounded_in_every_position() {
        const MIB: usize = 1024 * 1024;
        for filler in ["K", "\u{1}", "\u{10ffff}"] {
            let huge: Arc<str> = filler.repeat(MIB / filler.len()).into();
            let mut cases: Vec<MethodologyDef> = Vec::new();
            let mut d = base();
            d.key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.impact_categories[1].key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.impact_categories[0].levels[2].key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.feasibility_levels[2].key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.risk_levels[1].key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.impact_categories[0].risk_matrix[0][0] = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.properties[2].key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.threat_categories[1].key = Arc::clone(&huge);
            cases.push(d);
            let mut d = base();
            d.catalog_schemes[0].key = Arc::clone(&huge);
            cases.push(d);
            // An unknown `violates` entry is looked up, never key-checked.
            let mut d = base();
            d.threat_categories[0].violates = vec![Arc::clone(&huge)];
            cases.push(d);
            let mut d = base();
            d.threat_categories[0].violates = vec!["x".repeat(MIB).into()];
            cases.push(d);
            let mut d = base();
            let mut ap = sample_attack_potential();
            ap.factors[0].key = Arc::clone(&huge);
            d.attack_potential = Some(ap);
            cases.push(d);
            let mut d = base();
            let mut ap = sample_attack_potential();
            ap.factors[0].levels[1].key = Arc::clone(&huge);
            d.attack_potential = Some(ap);
            cases.push(d);
            let mut d = base();
            let mut ap = sample_attack_potential();
            ap.bands[0].feasibility = Arc::clone(&huge);
            d.attack_potential = Some(ap);
            cases.push(d);
            for (i, d) in cases.into_iter().enumerate() {
                let (e, k) = key_error(d);
                assert!(k.len() <= MAX_KEY_LEN, "case {i}: {} bytes", k.len());
                assert!(k.ends_with("..."), "case {i}");
                assert!(
                    e.to_string().len() <= 512,
                    "case {i}: {}",
                    e.to_string().len()
                );
            }
        }
    }

    #[test]
    fn methodology_error_key_text_reports_the_right_variant() {
        let huge: Arc<str> = "K".repeat(100_000).into();
        let cut: Arc<str> = format!("{}...", "K".repeat(61)).into();
        let mut d = base();
        d.properties[0].key = Arc::clone(&huge);
        rejects(d, &MethodologyError::InvalidKey(Arc::clone(&cut)));
        let mut d = base();
        d.threat_categories[1].violates.push(Arc::clone(&huge));
        rejects(
            d,
            &MethodologyError::UnknownViolatedProperty {
                category: "wipe".into(),
                property: cut,
            },
        );
    }

    #[test]
    fn methodology_error_key_text_never_splits_a_char() {
        // 4-byte chars: 15 of them fill 60 bytes, the 16th would pass 61.
        let emoji = "\u{1f600}";
        let cases = [
            (emoji.repeat(1000), format!("{}...", emoji.repeat(15))),
            (
                format!("a{}", emoji.repeat(100)),
                format!("a{}...", emoji.repeat(15)),
            ),
            (
                format!("ab{}", emoji.repeat(100)),
                format!("ab{}...", emoji.repeat(14)),
            ),
            (
                format!("abc{}", emoji.repeat(100)),
                format!("abc{}...", emoji.repeat(14)),
            ),
            (
                format!("é{}", "x".repeat(100)),
                format!("é{}...", "x".repeat(59)),
            ),
        ];
        for (key, want) in cases {
            let k = error_key_text(&Arc::from(key.as_str()));
            assert_eq!(&*k, want.as_str());
            assert!(k.len() <= MAX_KEY_LEN);
        }
    }

    #[test]
    fn methodology_error_key_text_keeps_short_keys_exactly() {
        for len in [0, 1, 9, 63, 64] {
            let key: Arc<str> = "A".repeat(len).into();
            let kept = error_key_text(&key);
            assert!(
                Arc::ptr_eq(&kept, &key),
                "{len} bytes are shared, not copied"
            );
        }
        let over: Arc<str> = "A".repeat(65).into();
        assert_eq!(&*error_key_text(&over), format!("{}...", "A".repeat(61)));
        // The 64-byte invalid key in an error is the whole key.
        let mut d = base();
        d.key = "A".repeat(64).into();
        let (_, k) = key_error(d);
        assert_eq!(&*k, "A".repeat(64));
        // Unchanged for the existing short-key cases.
        let mut d = base();
        d.key = "Not-A-Key".into();
        rejects(d, &MethodologyError::InvalidKey("Not-A-Key".into()));
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
    fn attack_potential_validation_and_resolution() {
        let mut d = base();
        d.attack_potential = Some(sample_attack_potential());
        let m = Methodology::new(d).expect("valid attack potential");
        assert_eq!(m.feasibility_factor_count(), 2);
        assert_eq!(m.feasibility_factors().len(), 2);
        assert_eq!(m.potential_bands().len(), 3);

        let time = m.feasibility_factor("time").expect("time");
        let skill = m.feasibility_factor("skill").expect("skill");
        assert_eq!(m.feasibility_factor_key(time), Some("time"));
        assert_eq!(m.feasibility_factor_label(time), Some("time"));
        assert_eq!(m.factor_level_count(time), 2);
        assert_eq!(m.factor_level_count(FeasibilityFactor(9)), 0);
        assert_eq!(m.factor_levels(FeasibilityFactor(9)).count(), 0);

        let fast = m.factor_level(time, "fast").expect("fast");
        let slow = m.factor_level(time, "slow").expect("slow");
        let low = m.factor_level(skill, "low").expect("low");
        let high = m.factor_level(skill, "high").expect("high");
        assert_eq!(m.factor_level_key(time, slow), Some("slow"));
        assert_eq!(m.factor_level_label(time, slow), Some("slow"));
        assert_eq!(m.factor_level_points(time, slow), Some(5));
        assert_eq!(m.factor_level_points(FeasibilityFactor(9), slow), None);
        assert_eq!(m.factor_level_points(time, FactorLevel(9)), None);

        assert_eq!(m.check_feasibility_factor(time), Ok(()));
        assert_eq!(
            m.check_feasibility_factor(FeasibilityFactor(2)),
            Err(ModelError::UnknownFeasibilityFactor(FeasibilityFactor(2)))
        );
        assert_eq!(m.check_factor_level(time, slow), Ok(()));
        assert_eq!(
            m.check_factor_level(time, FactorLevel(2)),
            Err(ModelError::UnknownFactorLevel {
                factor: time,
                level: FactorLevel(2),
            })
        );

        let ap0 = m
            .resolve_attack_potential(&[Some(fast), Some(low)])
            .expect("resolved");
        assert_eq!(ap0.score(), 0);
        assert_eq!(ap0.feasibility(), FeasibilityLevel(2));
        assert_eq!(ap0.band().min(), 0);
        assert_eq!(ap0.band().max(), Some(4));
        assert_eq!(ap0.band().feasibility(), FeasibilityLevel(2));
        assert!(ap0.band().contains(4));
        assert!(!ap0.band().contains(5));
        assert_eq!(ap0.factors().len(), 2);
        assert_eq!(ap0.factors()[0].factor(), time);
        assert_eq!(ap0.factors()[0].level(), fast);
        assert_eq!(ap0.factors()[0].points(), 0);

        let ap5 = m
            .resolve_attack_potential(&[Some(slow), Some(low)])
            .expect("resolved");
        assert_eq!(ap5.score(), 5);
        assert_eq!(ap5.feasibility(), FeasibilityLevel(1));

        let ap12 = m
            .resolve_attack_potential(&[Some(slow), Some(high)])
            .expect("resolved");
        assert_eq!(ap12.score(), 12);
        assert_eq!(ap12.feasibility(), FeasibilityLevel(0));
        assert_eq!(ap12.band().max(), None);
        assert!(ap12.band().contains(999_999));

        assert!(m.resolve_attack_potential(&[Some(fast), None]).is_none());
        assert!(m.resolve_attack_potential(&[Some(fast)]).is_none());
        assert!(
            m.resolve_attack_potential(&[Some(FactorLevel(9)), Some(low)])
                .is_none()
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn methodology_rejects_invalid_attack_potential() {
        // Empty factors
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.factors.clear();
        d.attack_potential = Some(ap);
        rejects(
            d,
            &MethodologyError::FeasibilityFactorCount { min: 1, max: 16 },
        );

        // 17 factors
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.factors = (0..17)
            .map(|i| factor_def(&format!("f{i}"), &[("a", 0), ("b", 1)]))
            .collect();
        d.attack_potential = Some(ap);
        rejects(
            d,
            &MethodologyError::FeasibilityFactorCount { min: 1, max: 16 },
        );

        // Duplicate factor
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.factors.push(factor_def("time", &[("a", 0), ("b", 1)]));
        d.attack_potential = Some(ap);
        rejects(
            d,
            &MethodologyError::DuplicateFeasibilityFactor("time".into()),
        );

        // Factor level count out of 2..=16
        for len in [0, 1, 17] {
            let mut d = base();
            let mut ap = sample_attack_potential();
            ap.factors[0].levels = (0..len)
                .map(|i| factor_level_def(&format!("l{i}"), 1))
                .collect();
            d.attack_potential = Some(ap);
            rejects(
                d,
                &MethodologyError::FactorLevelCount {
                    factor: "time".into(),
                    min: 2,
                    max: 16,
                },
            );
        }

        // Duplicate factor level
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.factors[0].levels.push(factor_level_def("fast", 2));
        d.attack_potential = Some(ap);
        rejects(
            d,
            &MethodologyError::DuplicateFactorLevel {
                factor: "time".into(),
                level: "fast".into(),
            },
        );

        // Factor points > MAX_FACTOR_POINTS
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.factors[0].levels[1].points = MAX_FACTOR_POINTS + 1;
        d.attack_potential = Some(ap);
        rejects(
            d,
            &MethodologyError::FactorPointsTooLarge {
                factor: "time".into(),
                level: "slow".into(),
                max: MAX_FACTOR_POINTS,
            },
        );

        // Band count out of 1..=16
        for len in [0, 17] {
            let mut d = base();
            let mut ap = sample_attack_potential();
            ap.bands = (0..len)
                .map(|i| {
                    let min = u16::try_from(i).unwrap_or(0);
                    let max = (i + 1 < len).then_some(min);
                    band_def(min, max, "f0")
                })
                .collect();
            d.attack_potential = Some(ap);
            rejects(d, &MethodologyError::PotentialBandCount { min: 1, max: 16 });
        }

        // Band 0 min != 0
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[0].min = 1;
        d.attack_potential = Some(ap);
        rejects(d, &MethodologyError::PotentialBandRange { band: 0 });

        // Band gap: band 0 ends at 3, band 1 starts at 5
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[0].max = Some(3);
        d.attack_potential = Some(ap);
        rejects(d, &MethodologyError::PotentialBandRange { band: 1 });

        // Band overlap: band 0 ends at 5, band 1 starts at 5
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[0].max = Some(5);
        d.attack_potential = Some(ap);
        rejects(d, &MethodologyError::PotentialBandRange { band: 1 });

        // Band min > max
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[1].min = 11;
        d.attack_potential = Some(ap);
        rejects(d, &MethodologyError::PotentialBandRange { band: 1 });

        // Non-last band has max == None
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[0].max = None;
        d.attack_potential = Some(ap);
        rejects(d, &MethodologyError::PotentialBandRange { band: 0 });

        // Last band has max == Some(...)
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[2].max = Some(99);
        d.attack_potential = Some(ap);
        rejects(d, &MethodologyError::PotentialBandRange { band: 2 });

        // Unknown band feasibility level
        let mut d = base();
        let mut ap = sample_attack_potential();
        ap.bands[1].feasibility = "f9".into();
        d.attack_potential = Some(ap);
        rejects(
            d,
            &MethodologyError::UnknownBandFeasibilityLevel {
                band: 1,
                feasibility: "f9".into(),
            },
        );
    }

    #[test]
    fn element_names_validation_and_plain_likelihood() {
        let m = Methodology::new(base()).expect("valid");
        assert!(!m.has_attack_potential());
        assert_eq!(m.feasibility_label(), "Attack feasibility");
        assert_eq!(
            &*m.element_names().damage_scenario.singular,
            "Damage scenario"
        );

        let mut custom = base();
        custom.element_names.damage_scenario = ElementNameDef::new("Harm", "Harms");
        custom.element_names.cybersecurity_goal =
            ElementNameDef::new("Security requirement", "Security requirements");
        custom.element_names.feasibility = "Likelihood".into();
        let m_custom = Methodology::new(custom).expect("valid custom names");
        assert_eq!(&*m_custom.element_names().damage_scenario.singular, "Harm");
        assert_eq!(&*m_custom.element_names().damage_scenario.plural, "Harms");
        assert_eq!(m_custom.feasibility_label(), "Likelihood");

        let mut blank = base();
        blank.element_names.damage_scenario.singular = "   ".into();
        rejects(
            blank,
            &MethodologyError::BlankLabel {
                key: "damage_scenario".into(),
            },
        );

        let mut blank_feas = base();
        blank_feas.element_names.feasibility = "".into();
        rejects(
            blank_feas,
            &MethodologyError::BlankLabel {
                key: "feasibility".into(),
            },
        );

        let mut long = base();
        long.element_names.asset.plural = "A".repeat(MAX_LABEL_CHARS + 1).into();
        rejects(
            long,
            &MethodologyError::LabelTooLong {
                key: "asset".into(),
                max: MAX_LABEL_CHARS,
            },
        );
    }

    #[test]
    fn value_sizes() {
        use std::mem::size_of;
        assert_eq!(size_of::<ImpactCategory>(), 1);
        assert_eq!(size_of::<Level>(), 1);
        assert_eq!(size_of::<FeasibilityLevel>(), 1);
        assert_eq!(size_of::<RiskLevel>(), 1);
        assert_eq!(size_of::<FeasibilityFactor>(), 1);
        assert_eq!(size_of::<FactorLevel>(), 1);
        assert_eq!(size_of::<Property>(), 1);
        assert_eq!(size_of::<ThreatCategory>(), 1);
        assert_eq!(size_of::<CatalogScheme>(), 1);
        assert_eq!(size_of::<InterfaceKind>(), 1);
        assert_eq!(InterfaceKind::new(3).index(), 3);
        assert_eq!(size_of::<AttackType>(), 1);
        assert_eq!(AttackType::new(3).index(), 3);
        assert_eq!(size_of::<PropertySet>(), 8);
        assert!(size_of::<CatalogRef>() <= 12, "{}", size_of::<CatalogRef>());
    }
}
