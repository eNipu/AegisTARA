//! Methodology pack files: format `aegis-methodology-pack` version 1.
//!
//! A pack is one JSON object that holds exactly a [`MethodologyDef`] (see
//! `docs/adr/0011-methodology-pack-files.md`):
//!
//! ```text
//! {"format": "aegis-methodology-pack", "version": 1, "methodology": {...}}
//! ```
//!
//! Reading decodes into private `*Wire` types whose leaves are bounded
//! (`BoundedStr`, `BoundedVec`, shared with the project store): every
//! string and every list is checked against its cap inside the serde
//! visitor, and the whole input against [`MAX_PACK_FILE_BYTES`], before
//! [`Methodology::new`] runs. The wire types then move their `Arc<str>`
//! values into the public `*Def` types, which have no serde derives.
//! Writing serializes borrowed views straight from the definition with the
//! pretty printer, in the declared field order, so the same definition
//! always gives the same bytes (P4).

use std::collections::BTreeMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;

use serde::de::{Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeStruct, Serializer};

use super::{
    AttackPotentialDef, CatalogSchemeDef, CatalogSyntax, ControlKindDef, ElementNameDef,
    ElementNamesDef, FactorDef, FactorLevelDef, ImpactCategoryDef, LevelDef, MAX_ATTACK_TYPES,
    MAX_CATALOG_SCHEMES, MAX_CONTROL_KINDS, MAX_FEASIBILITY_FACTORS, MAX_IMPACT_CATEGORIES,
    MAX_INTERFACE_KINDS, MAX_LEVELS, MAX_PACK_CLAUSE_REFS, MAX_PACK_TRANSLATIONS, MAX_PREFIX_LEN,
    MAX_PROPERTIES, MAX_SEGMENTS, MAX_THREAT_CATEGORIES, Methodology, MethodologyDef,
    PackTranslationDef, PotentialBandDef, PropertyDef, SegmentDef, ThreatCategoryDef,
};
use crate::error::PackError;
use crate::store::wire::{
    self, BoundedKeyMap, BoundedStr, BoundedVec, Cap, CappedReader, FORMAT_VERSION, Format,
    FormatName, FormatVersion, Key, VersionText,
};
use crate::store::{Limit, MAX_LABEL_BYTES, MAX_PACK_FILE_BYTES};

/// The pack format name.
const PACK_FORMAT: &str = "aegis-methodology-pack";

// ---------------------------------------------------------------------------
// Caps.
// ---------------------------------------------------------------------------

/// Defines a cap type for a pack list or string.
macro_rules! pack_caps {
    ($($name:ident = $max:expr, $limit:expr;)*) => {$(
        enum $name {}
        impl Cap for $name {
            const MAX: usize = $max;
            const LIMIT: Limit = $limit;
        }
    )*};
}

pack_caps! {
    LabelCap = MAX_LABEL_BYTES, Limit::LABEL;
    PrefixCap = MAX_PREFIX_LEN, Limit::CATALOG_PREFIX;
    ImpactCategoriesCap = MAX_IMPACT_CATEGORIES, Limit::IMPACT_CATEGORIES;
    LevelsCap = MAX_LEVELS, Limit::LEVELS;
    ImpactLevelsMapCap = MAX_IMPACT_CATEGORIES * MAX_LEVELS, Limit::LEVELS;
    FeasibilityFactorsCap = MAX_FEASIBILITY_FACTORS, Limit::FEASIBILITY_FACTORS;
    FactorLevelsMapCap = MAX_FEASIBILITY_FACTORS * MAX_LEVELS, Limit::LEVELS;
    PropertiesCap = MAX_PROPERTIES, Limit::PROPERTIES;
    ThreatCategoriesCap = MAX_THREAT_CATEGORIES, Limit::THREAT_CATEGORIES;
    CatalogSchemesCap = MAX_CATALOG_SCHEMES, Limit::CATALOG_SCHEMES;
    SegmentsCap = MAX_SEGMENTS, Limit::CATALOG_SEGMENTS;
    InterfaceKindsCap = MAX_INTERFACE_KINDS, Limit::INTERFACE_KINDS;
    AttackTypesCap = MAX_ATTACK_TYPES, Limit::ATTACK_TYPES;
    ControlKindsCap = MAX_CONTROL_KINDS, Limit::CONTROL_KINDS;
    ControlKindConflictsCap = MAX_CONTROL_KINDS, Limit::CONTROL_KIND_CONFLICTS;
    ControlKindRulesCap = crate::control::MAX_CONTROL_RULES, Limit::CONTROL_RULES;
    PackTranslationsCap = MAX_PACK_TRANSLATIONS, Limit::PACK_TRANSLATIONS;
    ClauseRefsCap = MAX_PACK_CLAUSE_REFS, Limit::CLAUSE_REFS;
}

type Label = BoundedStr<LabelCap>;

/// The pack format.
enum PackFormat {}
impl FormatName for PackFormat {
    const NAME: &'static str = PACK_FORMAT;
}

// ---------------------------------------------------------------------------
// Objects only.
// ---------------------------------------------------------------------------

/// A struct that must be a JSON object. Serde's derived struct visitor also
/// accepts a JSON array of the field values in order; a pack never uses
/// that form, so it is a shape error ([`PackError::Decode`]).
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(MapOnly(d)).map(Object)
    }
}

/// Forwards to the inner deserializer, but asks for a map where a struct is
/// requested. Time O(1) besides the inner call.
struct MapOnly<D>(D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for MapOnly<D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, D::Error> {
        self.0.deserialize_any(v)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, D::Error> {
        self.0.deserialize_map(v)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

// ---------------------------------------------------------------------------
// Owned wire types (reading).
// ---------------------------------------------------------------------------

/// A whole pack. The format fields are checked as soon as they are read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PackWire {
    #[serde(rename = "format")]
    _format: Format<PackFormat>,
    #[serde(rename = "version")]
    _version: FormatVersion,
    methodology: Object<DefWire>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DefWire {
    key: Key,
    version: VersionText,
    label: Label,
    #[serde(default)]
    element_names: Option<Object<ElementNamesWire>>,
    #[serde(default)]
    interface_kinds: Option<BoundedVec<Object<EntryWire>, InterfaceKindsCap>>,
    #[serde(default)]
    attack_types: Option<BoundedVec<Object<EntryWire>, AttackTypesCap>>,
    #[serde(default)]
    control_kinds: Option<BoundedVec<Object<ControlKindWire>, ControlKindsCap>>,
    impact_categories: BoundedVec<Object<ImpactCategoryWire>, ImpactCategoriesCap>,
    feasibility_levels: BoundedVec<Object<EntryWire>, LevelsCap>,
    risk_levels: BoundedVec<Object<EntryWire>, LevelsCap>,
    #[serde(default)]
    attack_potential: Option<Object<AttackPotentialWire>>,
    #[serde(default)]
    require_rationale: bool,
    #[serde(default)]
    clause_refs: Option<BoundedKeyMap<BoundedVec<Label, ClauseRefsCap>, ClauseRefsCap>>,
    properties: BoundedVec<Object<EntryWire>, PropertiesCap>,
    threat_categories: BoundedVec<Object<ThreatCategoryWire>, ThreatCategoriesCap>,
    catalog_schemes: BoundedVec<Object<CatalogSchemeWire>, CatalogSchemesCap>,
    #[serde(default)]
    translations: Option<BoundedKeyMap<Object<PackTranslationWire>, PackTranslationsCap>>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ElementNameWire {
    singular: Label,
    plural: Label,
}

#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ElementNamesWire {
    #[serde(default)]
    component: Option<Object<ElementNameWire>>,
    #[serde(default)]
    interface: Option<Object<ElementNameWire>>,
    #[serde(default)]
    data_flow: Option<Object<ElementNameWire>>,
    #[serde(default)]
    trust_boundary: Option<Object<ElementNameWire>>,
    #[serde(default)]
    asset: Option<Object<ElementNameWire>>,
    #[serde(default)]
    damage_scenario: Option<Object<ElementNameWire>>,
    #[serde(default)]
    threat_scenario: Option<Object<ElementNameWire>>,
    #[serde(default)]
    attack_tree: Option<Object<ElementNameWire>>,
    #[serde(default)]
    security_control: Option<Object<ElementNameWire>>,
    #[serde(default)]
    cybersecurity_goal: Option<Object<ElementNameWire>>,
    #[serde(default)]
    cybersecurity_claim: Option<Object<ElementNameWire>>,
    #[serde(default)]
    feasibility: Option<Label>,
}

#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PackTranslationWire {
    #[serde(default)]
    label: Option<Label>,
    #[serde(default)]
    element_names: Option<Object<ElementNamesWire>>,
    #[serde(default)]
    impact_categories: BoundedKeyMap<Label, ImpactCategoriesCap>,
    #[serde(default)]
    impact_levels: BoundedKeyMap<Label, ImpactLevelsMapCap>,
    #[serde(default)]
    feasibility_levels: BoundedKeyMap<Label, LevelsCap>,
    #[serde(default)]
    risk_levels: BoundedKeyMap<Label, LevelsCap>,
    #[serde(default)]
    properties: BoundedKeyMap<Label, PropertiesCap>,
    #[serde(default)]
    threat_categories: BoundedKeyMap<Label, ThreatCategoriesCap>,
    #[serde(default)]
    catalog_schemes: BoundedKeyMap<Label, CatalogSchemesCap>,
    #[serde(default)]
    feasibility_factors: BoundedKeyMap<Label, FeasibilityFactorsCap>,
    #[serde(default)]
    factor_levels: BoundedKeyMap<Label, FactorLevelsMapCap>,
    #[serde(default)]
    interface_kinds: BoundedKeyMap<Label, InterfaceKindsCap>,
    #[serde(default)]
    attack_types: BoundedKeyMap<Label, AttackTypesCap>,
    #[serde(default)]
    control_kinds: BoundedKeyMap<Label, ControlKindsCap>,
}

/// A setting preset: a key, a label, the interface kinds it is for, and rules.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlKindWire {
    key: Key,
    label: Label,
    #[serde(default)]
    interface_kinds: BoundedVec<Key, InterfaceKindsCap>,
    rules: BoundedKeyMap<Key, ControlKindRulesCap>,
    #[serde(default)]
    conflicts_with: BoundedVec<Key, ControlKindConflictsCap>,
}

/// A level or a property: a key and a label.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    key: Key,
    label: Label,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AttackPotentialWire {
    factors: BoundedVec<Object<FactorWire>, FeasibilityFactorsCap>,
    bands: BoundedVec<Object<PotentialBandWire>, LevelsCap>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FactorWire {
    key: Key,
    label: Label,
    levels: BoundedVec<Object<FactorLevelWire>, LevelsCap>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FactorLevelWire {
    key: Key,
    label: Label,
    points: u16,
}

/// One score band. `max` may be missing or `null`; both mean `None`
/// (unbounded upper end).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PotentialBandWire {
    min: u16,
    #[serde(default)]
    max: Option<u16>,
    feasibility: Key,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ImpactCategoryWire {
    key: Key,
    label: Label,
    levels: BoundedVec<Object<EntryWire>, LevelsCap>,
    risk_matrix: BoundedVec<BoundedVec<Key, LevelsCap>, LevelsCap>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ThreatCategoryWire {
    key: Key,
    label: Label,
    violates: BoundedVec<Key, PropertiesCap>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSchemeWire {
    key: Key,
    label: Label,
    syntax: Object<SyntaxWire>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntaxWire {
    prefix: BoundedStr<PrefixCap>,
    required: u8,
    segments: BoundedVec<Object<SegmentWire>, SegmentsCap>,
}

/// One segment. `digits` may be missing or `null`; both mean `None`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentWire {
    min: u16,
    max: u16,
    #[serde(default)]
    digits: Option<u8>,
}

impl From<AttackPotentialWire> for AttackPotentialDef {
    fn from(ap: AttackPotentialWire) -> Self {
        AttackPotentialDef {
            factors: ap
                .factors
                .0
                .into_iter()
                .map(|Object(f)| FactorDef {
                    key: f.key.0,
                    label: f.label.0,
                    levels: f
                        .levels
                        .0
                        .into_iter()
                        .map(|Object(fl)| FactorLevelDef {
                            key: fl.key.0,
                            label: fl.label.0,
                            points: fl.points,
                        })
                        .collect(),
                })
                .collect(),
            bands: ap
                .bands
                .0
                .into_iter()
                .map(|Object(b)| PotentialBandDef {
                    min: b.min,
                    max: b.max,
                    feasibility: b.feasibility.0,
                })
                .collect(),
        }
    }
}

impl From<ElementNameWire> for ElementNameDef {
    fn from(w: ElementNameWire) -> Self {
        ElementNameDef {
            singular: w.singular.0,
            plural: w.plural.0,
        }
    }
}

impl From<ElementNamesWire> for ElementNamesDef {
    fn from(w: ElementNamesWire) -> Self {
        let def = ElementNamesDef::default();
        ElementNamesDef {
            component: w.component.map_or(def.component, |Object(n)| n.into()),
            interface: w.interface.map_or(def.interface, |Object(n)| n.into()),
            data_flow: w.data_flow.map_or(def.data_flow, |Object(n)| n.into()),
            trust_boundary: w
                .trust_boundary
                .map_or(def.trust_boundary, |Object(n)| n.into()),
            asset: w.asset.map_or(def.asset, |Object(n)| n.into()),
            damage_scenario: w
                .damage_scenario
                .map_or(def.damage_scenario, |Object(n)| n.into()),
            threat_scenario: w
                .threat_scenario
                .map_or(def.threat_scenario, |Object(n)| n.into()),
            attack_tree: w.attack_tree.map_or(def.attack_tree, |Object(n)| n.into()),
            security_control: w
                .security_control
                .map_or(def.security_control, |Object(n)| n.into()),
            cybersecurity_goal: w
                .cybersecurity_goal
                .map_or(def.cybersecurity_goal, |Object(n)| n.into()),
            cybersecurity_claim: w
                .cybersecurity_claim
                .map_or(def.cybersecurity_claim, |Object(n)| n.into()),
            feasibility: w.feasibility.map_or(def.feasibility, |l| l.0),
            interface_kinds: Vec::new(),
            attack_types: Vec::new(),
            control_kinds: Vec::new(),
            translations: BTreeMap::new(),
        }
    }
}

fn unwrap_label_map<C>(m: BoundedKeyMap<Label, C>) -> BTreeMap<Arc<str>, Arc<str>> {
    m.0.into_iter().map(|(k, v)| (k, v.0)).collect()
}

impl From<PackTranslationWire> for PackTranslationDef {
    fn from(w: PackTranslationWire) -> Self {
        PackTranslationDef {
            label: w.label.map(|l| l.0),
            element_names: w.element_names.map(|Object(n)| n.into()),
            impact_categories: unwrap_label_map(w.impact_categories),
            impact_levels: unwrap_label_map(w.impact_levels),
            feasibility_levels: unwrap_label_map(w.feasibility_levels),
            risk_levels: unwrap_label_map(w.risk_levels),
            properties: unwrap_label_map(w.properties),
            threat_categories: unwrap_label_map(w.threat_categories),
            catalog_schemes: unwrap_label_map(w.catalog_schemes),
            feasibility_factors: unwrap_label_map(w.feasibility_factors),
            factor_levels: unwrap_label_map(w.factor_levels),
            interface_kinds: unwrap_label_map(w.interface_kinds),
            attack_types: unwrap_label_map(w.attack_types),
            control_kinds: unwrap_label_map(w.control_kinds),
        }
    }
}

impl From<ControlKindWire> for ControlKindDef {
    fn from(k: ControlKindWire) -> Self {
        ControlKindDef {
            key: k.key.0,
            label: k.label.0,
            interface_kinds: k.interface_kinds.0.into_iter().map(|i| i.0).collect(),
            rules: k.rules.0.into_iter().map(|(a, l)| (a, l.0)).collect(),
            conflicts_with: k.conflicts_with.0.into_iter().map(|i| i.0).collect(),
        }
    }
}

/// Moves the decoded values into a definition: no string is copied, only
/// the `Arc<str>` handles move. Time O(m), m = vocabulary entries.
#[allow(clippy::too_many_lines)]
impl From<DefWire> for MethodologyDef {
    fn from(w: DefWire) -> Self {
        let entry = |e: Object<EntryWire>| LevelDef {
            key: e.0.key.0,
            label: e.0.label.0,
        };
        let mut element_names = w
            .element_names
            .map_or_else(ElementNamesDef::default, |Object(names)| names.into());
        if let Some(kinds) = w.interface_kinds {
            element_names.interface_kinds = kinds.0.into_iter().map(entry).collect();
        }
        if let Some(types) = w.attack_types {
            element_names.attack_types = types.0.into_iter().map(entry).collect();
        }
        if let Some(kinds) = w.control_kinds {
            element_names.control_kinds = kinds.0.into_iter().map(|Object(k)| k.into()).collect();
        }
        if let Some(translations) = w.translations {
            element_names.translations = translations
                .0
                .into_iter()
                .map(|(lang, Object(tr))| (lang, tr.into()))
                .collect();
        }
        MethodologyDef {
            key: w.key.0,
            version: w.version.0,
            label: w.label.0,
            element_names,
            impact_categories: w
                .impact_categories
                .0
                .into_iter()
                .map(|Object(c)| ImpactCategoryDef {
                    key: c.key.0,
                    label: c.label.0,
                    levels: c.levels.0.into_iter().map(entry).collect(),
                    risk_matrix: c
                        .risk_matrix
                        .0
                        .into_iter()
                        .map(|row| row.0.into_iter().map(|k| k.0).collect())
                        .collect(),
                })
                .collect(),
            feasibility_levels: w.feasibility_levels.0.into_iter().map(entry).collect(),
            risk_levels: w.risk_levels.0.into_iter().map(entry).collect(),
            attack_potential: w.attack_potential.map(|Object(ap)| ap.into()),
            require_rationale: w.require_rationale,
            clause_refs: w.clause_refs.map_or_default(|m| {
                m.0.into_iter()
                    .map(|(k, v)| (k, v.0.into_iter().map(|l| l.0).collect()))
                    .collect()
            }),
            properties: w
                .properties
                .0
                .into_iter()
                .map(|Object(e)| PropertyDef {
                    key: e.key.0,
                    label: e.label.0,
                })
                .collect(),
            threat_categories: w
                .threat_categories
                .0
                .into_iter()
                .map(|Object(t)| ThreatCategoryDef {
                    key: t.key.0,
                    label: t.label.0,
                    violates: t.violates.0.into_iter().map(|k| k.0).collect(),
                })
                .collect(),
            catalog_schemes: w
                .catalog_schemes
                .0
                .into_iter()
                .map(|Object(s)| CatalogSchemeDef {
                    key: s.key.0,
                    label: s.label.0,
                    syntax: CatalogSyntax {
                        prefix: s.syntax.0.prefix.0,
                        segments: s
                            .syntax
                            .0
                            .segments
                            .0
                            .into_iter()
                            .map(|Object(g)| SegmentDef {
                                min: g.min,
                                max: g.max,
                                digits: g.digits,
                            })
                            .collect(),
                        required: s.syntax.0.required,
                    },
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Borrowed views (writing).
// ---------------------------------------------------------------------------

/// The whole pack, borrowed from the definition.
#[derive(serde::Serialize)]
struct PackView<'a> {
    format: &'static str,
    version: u32,
    methodology: View<'a, MethodologyDef>,
}

/// A borrowed part of a definition, serialized in the pack's field order.
struct View<'a, T: ?Sized>(&'a T);

impl<T> Serialize for View<'_, [T]>
where
    for<'b> View<'b, T>: Serialize,
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(View))
    }
}

impl Serialize for View<'_, [Arc<str>]> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|k| &**k))
    }
}

impl Serialize for View<'_, Vec<Arc<str>>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        View(self.0.as_slice()).serialize(s)
    }
}

impl Serialize for View<'_, MethodologyDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let d = self.0;
        let mut st = s.serialize_struct("methodology", 16)?;
        st.serialize_field("key", &*d.key)?;
        st.serialize_field("version", &*d.version)?;
        st.serialize_field("label", &*d.label)?;
        if d.element_names.has_custom_names() {
            st.serialize_field("element_names", &View(&d.element_names))?;
        } else {
            st.skip_field("element_names")?;
        }
        st.serialize_field("impact_categories", &View(d.impact_categories.as_slice()))?;
        st.serialize_field("feasibility_levels", &View(d.feasibility_levels.as_slice()))?;
        st.serialize_field("risk_levels", &View(d.risk_levels.as_slice()))?;
        match &d.attack_potential {
            Some(ap) => st.serialize_field("attack_potential", &View(ap))?,
            None => st.skip_field("attack_potential")?,
        }
        if d.require_rationale {
            st.serialize_field("require_rationale", &true)?;
        } else {
            st.skip_field("require_rationale")?;
        }
        if d.clause_refs.is_empty() {
            st.skip_field("clause_refs")?;
        } else {
            st.serialize_field("clause_refs", &View(&d.clause_refs))?;
        }
        st.serialize_field("properties", &View(d.properties.as_slice()))?;
        st.serialize_field("threat_categories", &View(d.threat_categories.as_slice()))?;
        st.serialize_field("catalog_schemes", &View(d.catalog_schemes.as_slice()))?;
        if d.element_names.interface_kinds.is_empty() {
            st.skip_field("interface_kinds")?;
        } else {
            st.serialize_field(
                "interface_kinds",
                &View(d.element_names.interface_kinds.as_slice()),
            )?;
        }
        if d.element_names.attack_types.is_empty() {
            st.skip_field("attack_types")?;
        } else {
            st.serialize_field(
                "attack_types",
                &View(d.element_names.attack_types.as_slice()),
            )?;
        }
        if d.element_names.control_kinds.is_empty() {
            st.skip_field("control_kinds")?;
        } else {
            st.serialize_field(
                "control_kinds",
                &View(d.element_names.control_kinds.as_slice()),
            )?;
        }
        if d.element_names.translations.is_empty() {
            st.skip_field("translations")?;
        } else {
            st.serialize_field("translations", &View(&d.element_names.translations))?;
        }
        st.end()
    }
}

impl Serialize for View<'_, ControlKindDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let k = self.0;
        let mut st = s.serialize_struct("control_kind", 5)?;
        st.serialize_field("key", &*k.key)?;
        st.serialize_field("label", &*k.label)?;
        if k.interface_kinds.is_empty() {
            st.skip_field("interface_kinds")?;
        } else {
            st.serialize_field("interface_kinds", &View(&k.interface_kinds))?;
        }
        st.serialize_field("rules", &View(&k.rules))?;
        if k.conflicts_with.is_empty() {
            st.skip_field("conflicts_with")?;
        } else {
            st.serialize_field("conflicts_with", &View(&k.conflicts_with))?;
        }
        st.end()
    }
}

impl Serialize for View<'_, ElementNameDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let n = self.0;
        let mut st = s.serialize_struct("element_name", 2)?;
        st.serialize_field("singular", &*n.singular)?;
        st.serialize_field("plural", &*n.plural)?;
        st.end()
    }
}

impl Serialize for View<'_, ElementNamesDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let n = self.0;
        let mut st = s.serialize_struct("element_names", 12)?;
        if n.has_custom_item_names() {
            st.serialize_field("component", &View(&n.component))?;
            st.serialize_field("interface", &View(&n.interface))?;
            st.serialize_field("data_flow", &View(&n.data_flow))?;
            st.serialize_field("trust_boundary", &View(&n.trust_boundary))?;
        } else {
            st.skip_field("component")?;
            st.skip_field("interface")?;
            st.skip_field("data_flow")?;
            st.skip_field("trust_boundary")?;
        }
        st.serialize_field("asset", &View(&n.asset))?;
        st.serialize_field("damage_scenario", &View(&n.damage_scenario))?;
        st.serialize_field("threat_scenario", &View(&n.threat_scenario))?;
        st.serialize_field("attack_tree", &View(&n.attack_tree))?;
        st.serialize_field("security_control", &View(&n.security_control))?;
        st.serialize_field("cybersecurity_goal", &View(&n.cybersecurity_goal))?;
        st.serialize_field("cybersecurity_claim", &View(&n.cybersecurity_claim))?;
        st.serialize_field("feasibility", &*n.feasibility)?;
        st.end()
    }
}

impl Serialize for View<'_, BTreeMap<Arc<str>, PackTranslationDef>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (lang, tr) in self.0 {
            map.serialize_entry(&**lang, &View(tr))?;
        }
        map.end()
    }
}

impl Serialize for View<'_, BTreeMap<Arc<str>, Arc<str>>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in self.0 {
            map.serialize_entry(&**k, &**v)?;
        }
        map.end()
    }
}

impl Serialize for View<'_, BTreeMap<Arc<str>, Vec<Arc<str>>>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in self.0 {
            map.serialize_entry(&**k, &View(v.as_slice()))?;
        }
        map.end()
    }
}

impl Serialize for View<'_, PackTranslationDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let tr = self.0;
        let mut st = s.serialize_struct("pack_translation", 14)?;
        match &tr.label {
            Some(lbl) => st.serialize_field("label", &**lbl)?,
            None => st.skip_field("label")?,
        }
        match &tr.element_names {
            Some(names) => st.serialize_field("element_names", &View(names))?,
            None => st.skip_field("element_names")?,
        }
        if tr.impact_categories.is_empty() {
            st.skip_field("impact_categories")?;
        } else {
            st.serialize_field("impact_categories", &View(&tr.impact_categories))?;
        }
        if tr.impact_levels.is_empty() {
            st.skip_field("impact_levels")?;
        } else {
            st.serialize_field("impact_levels", &View(&tr.impact_levels))?;
        }
        if tr.feasibility_levels.is_empty() {
            st.skip_field("feasibility_levels")?;
        } else {
            st.serialize_field("feasibility_levels", &View(&tr.feasibility_levels))?;
        }
        if tr.risk_levels.is_empty() {
            st.skip_field("risk_levels")?;
        } else {
            st.serialize_field("risk_levels", &View(&tr.risk_levels))?;
        }
        if tr.properties.is_empty() {
            st.skip_field("properties")?;
        } else {
            st.serialize_field("properties", &View(&tr.properties))?;
        }
        if tr.threat_categories.is_empty() {
            st.skip_field("threat_categories")?;
        } else {
            st.serialize_field("threat_categories", &View(&tr.threat_categories))?;
        }
        if tr.catalog_schemes.is_empty() {
            st.skip_field("catalog_schemes")?;
        } else {
            st.serialize_field("catalog_schemes", &View(&tr.catalog_schemes))?;
        }
        if tr.feasibility_factors.is_empty() {
            st.skip_field("feasibility_factors")?;
        } else {
            st.serialize_field("feasibility_factors", &View(&tr.feasibility_factors))?;
        }
        if tr.factor_levels.is_empty() {
            st.skip_field("factor_levels")?;
        } else {
            st.serialize_field("factor_levels", &View(&tr.factor_levels))?;
        }
        if tr.interface_kinds.is_empty() {
            st.skip_field("interface_kinds")?;
        } else {
            st.serialize_field("interface_kinds", &View(&tr.interface_kinds))?;
        }
        if tr.attack_types.is_empty() {
            st.skip_field("attack_types")?;
        } else {
            st.serialize_field("attack_types", &View(&tr.attack_types))?;
        }
        if tr.control_kinds.is_empty() {
            st.skip_field("control_kinds")?;
        } else {
            st.serialize_field("control_kinds", &View(&tr.control_kinds))?;
        }
        st.end()
    }
}

impl Serialize for View<'_, AttackPotentialDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ap = self.0;
        let mut st = s.serialize_struct("attack_potential", 2)?;
        st.serialize_field("factors", &View(ap.factors.as_slice()))?;
        st.serialize_field("bands", &View(ap.bands.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, FactorDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let f = self.0;
        let mut st = s.serialize_struct("factor", 3)?;
        st.serialize_field("key", &*f.key)?;
        st.serialize_field("label", &*f.label)?;
        st.serialize_field("levels", &View(f.levels.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, FactorLevelDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let fl = self.0;
        let mut st = s.serialize_struct("factor_level", 3)?;
        st.serialize_field("key", &*fl.key)?;
        st.serialize_field("label", &*fl.label)?;
        st.serialize_field("points", &fl.points)?;
        st.end()
    }
}

impl Serialize for View<'_, PotentialBandDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let b = self.0;
        let mut st = s.serialize_struct("potential_band", 3)?;
        st.serialize_field("min", &b.min)?;
        match b.max {
            Some(m) => st.serialize_field("max", &m)?,
            None => st.skip_field("max")?,
        }
        st.serialize_field("feasibility", &*b.feasibility)?;
        st.end()
    }
}

impl Serialize for View<'_, ImpactCategoryDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let c = self.0;
        let mut st = s.serialize_struct("impact_category", 4)?;
        st.serialize_field("key", &*c.key)?;
        st.serialize_field("label", &*c.label)?;
        st.serialize_field("levels", &View(c.levels.as_slice()))?;
        st.serialize_field("risk_matrix", &View(c.risk_matrix.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, LevelDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        entry(s, "level", &self.0.key, &self.0.label)
    }
}

impl Serialize for View<'_, PropertyDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        entry(s, "property", &self.0.key, &self.0.label)
    }
}

/// `{"key": ..., "label": ...}`.
fn entry<S: Serializer>(
    s: S,
    name: &'static str,
    key: &str,
    label: &str,
) -> Result<S::Ok, S::Error> {
    let mut st = s.serialize_struct(name, 2)?;
    st.serialize_field("key", key)?;
    st.serialize_field("label", label)?;
    st.end()
}

impl Serialize for View<'_, ThreatCategoryDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let t = self.0;
        let mut st = s.serialize_struct("threat_category", 3)?;
        st.serialize_field("key", &*t.key)?;
        st.serialize_field("label", &*t.label)?;
        st.serialize_field("violates", &View(t.violates.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, CatalogSchemeDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let c = self.0;
        let mut st = s.serialize_struct("catalog_scheme", 3)?;
        st.serialize_field("key", &*c.key)?;
        st.serialize_field("label", &*c.label)?;
        st.serialize_field("syntax", &View(&c.syntax))?;
        st.end()
    }
}

impl Serialize for View<'_, CatalogSyntax> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        let mut st = s.serialize_struct("syntax", 3)?;
        st.serialize_field("prefix", &*x.prefix)?;
        st.serialize_field("required", &x.required)?;
        st.serialize_field("segments", &View(x.segments.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, SegmentDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let g = self.0;
        let mut st = s.serialize_struct("segment", 3)?;
        st.serialize_field("min", &g.min)?;
        st.serialize_field("max", &g.max)?;
        match g.digits {
            Some(w) => st.serialize_field("digits", &w)?,
            None => st.skip_field("digits")?,
        }
        st.end()
    }
}

// ---------------------------------------------------------------------------
// Public API.
// ---------------------------------------------------------------------------

impl MethodologyDef {
    /// Decodes an `aegis-methodology-pack` v1 file: the format check, the
    /// decode caps, and the strict shape. It does not validate the
    /// definition; [`Methodology::read_pack`] does both.
    ///
    /// The input is read through a buffer, never more than
    /// [`MAX_PACK_FILE_BYTES`] + 1 bytes of it. Every string and list is
    /// checked against its cap ([`Limit`]) while it is decoded. Unknown,
    /// missing, repeated, and mistyped fields, numbers outside their
    /// field's range, and trailing data are errors. `digits` on a segment
    /// may be missing or `null` (both `None`); every other field is
    /// required.
    ///
    /// Time O(B), B = input bytes. Space O(B): at most 4 MiB of input, and
    /// at most about 1.4 MB of definition at the caps.
    ///
    /// # Errors
    ///
    /// [`PackError::Io`] if reading fails; [`PackError::LimitExceeded`] if
    /// a cap breaks; [`PackError::UnsupportedFormat`] for a wrong format
    /// name or version; [`PackError::Decode`] for bad JSON or a wrong
    /// shape.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyDef};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let text = r#"{
    ///   "format": "aegis-methodology-pack",
    ///   "version": 1,
    ///   "methodology": {
    ///     "key": "tiny", "version": "1", "label": "Tiny",
    ///     "impact_categories": [{ "key": "harm", "label": "Harm", "levels": [
    ///       { "key": "low", "label": "Low" }, { "key": "high", "label": "High" }],
    ///       "risk_matrix": [["r1", "r2"], ["r2", "r2"]] }],
    ///     "feasibility_levels": [
    ///       { "key": "low", "label": "Low" }, { "key": "high", "label": "High" }],
    ///     "risk_levels": [
    ///       { "key": "r1", "label": "Low" }, { "key": "r2", "label": "High" }],
    ///     "properties": [{ "key": "integrity", "label": "Integrity" }],
    ///     "threat_categories": [{ "key": "corruption", "label": "Corruption",
    ///                             "violates": ["integrity"] }],
    ///     "catalog_schemes": []
    ///   }
    /// }"#;
    /// let def = MethodologyDef::read_pack(text.as_bytes())?;
    /// assert_eq!(&*def.key, "tiny");
    /// assert_eq!(Methodology::new(def)?.properties().count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_pack<R: Read>(r: R) -> Result<MethodologyDef, PackError> {
        let mut input = CappedReader::new(r, MAX_PACK_FILE_BYTES);
        let decoded = wire::decode_pack(|| {
            let mut de = serde_json::Deserializer::from_reader(BufReader::new(&mut input));
            let pack = Object::<PackWire>::deserialize(&mut de)?;
            de.end()?;
            Ok(pack)
        });
        if input.exceeded() {
            return Err(PackError::LimitExceeded(Limit::PACK_FILE));
        }
        Ok(decoded?.0.methodology.0.into())
    }

    /// Writes the definition as canonical `aegis-methodology-pack` v1 JSON:
    /// the pretty printer (2-space indent), the format's field order,
    /// `digits` left out when `None`, non-ASCII as raw UTF-8, and a final
    /// `\n`. The same definition always gives the same bytes (P4). It does
    /// not validate the definition, but every definition that
    /// [`Methodology::new`] accepts reads back equal.
    ///
    /// Writes go through an internal `BufWriter`, flushed before return.
    /// Time O(m + text), m = vocabulary entries; it serializes straight
    /// from the definition, with no copy (extra space O(1) besides the
    /// buffer).
    ///
    /// # Errors
    ///
    /// Only [`PackError::Io`], if writing fails.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyDef};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let builtin = include_str!("../../packs/iso21434-r155.json");
    /// let def = MethodologyDef::read_pack(builtin.as_bytes())?;
    /// let mut out = Vec::new();
    /// def.write_pack(&mut out)?;
    /// assert_eq!(out, builtin.as_bytes());
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_pack<W: Write>(&self, w: W) -> Result<(), PackError> {
        let view = PackView {
            format: PACK_FORMAT,
            version: FORMAT_VERSION,
            methodology: View(self),
        };
        let mut out = BufWriter::new(w);
        serde_json::to_writer_pretty(&mut out, &view).map_err(std::io::Error::from)?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(())
    }
}

impl Methodology {
    /// Reads a methodology pack and validates it:
    /// [`MethodologyDef::read_pack`], then [`Methodology::new`].
    ///
    /// Time O(B + m log m + text), B = input bytes, m = vocabulary entries
    /// (at most 705). Space O(B).
    ///
    /// # Errors
    ///
    /// Every error of [`MethodologyDef::read_pack`], and
    /// [`PackError::Methodology`] if the definition breaks a methodology
    /// rule.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyError, PackError};
    ///
    /// let text = r#"{"format": "aegis-methodology-pack", "version": 1, "methodology": {
    ///   "key": "tiny", "version": "1", "label": "Tiny", "impact_categories": [],
    ///   "feasibility_levels": [], "risk_levels": [],
    ///   "properties": [], "threat_categories": [], "catalog_schemes": []}}"#;
    /// let err = Methodology::read_pack(text.as_bytes()).err();
    /// assert!(matches!(err, Some(PackError::Methodology(MethodologyError::NoImpactCategory))));
    /// ```
    pub fn read_pack<R: Read>(r: R) -> Result<Methodology, PackError> {
        Ok(Methodology::new(MethodologyDef::read_pack(r)?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"{"format":"aegis-methodology-pack","version":1,"methodology":"#;

    fn tiny(methodology: &str) -> String {
        format!("{HEAD}{methodology}}}")
    }

    #[test]
    fn object_rejects_array_form() {
        let text = format!(r#"{HEAD}["k","1","L",[],[],[],[],[],[]]}}"#);
        assert!(matches!(
            MethodologyDef::read_pack(text.as_bytes()),
            Err(PackError::Decode { line: 1, .. })
        ));
        let whole = r#"["aegis-methodology-pack",1,{}]"#;
        assert!(matches!(
            MethodologyDef::read_pack(whole.as_bytes()),
            Err(PackError::Decode { .. })
        ));
    }

    #[test]
    fn map_only_forwards_other_requests() {
        let mut de = serde_json::Deserializer::from_str("7");
        assert_eq!(u8::deserialize(MapOnly(&mut de)).ok(), Some(7));
        let mut de = serde_json::Deserializer::from_str(r#"{"a":1}"#);
        let any = serde_json::Value::deserialize(MapOnly(&mut de)).ok();
        assert_eq!(any.map(|v| v.to_string()), Some(r#"{"a":1}"#.into()));
    }

    #[test]
    fn digits_missing_null_or_set() {
        let seg = |digits: &str| {
            let text = tiny(&format!(
                r#"{{"key":"k","version":"1","label":"L","impact_categories":[],"feasibility_levels":[],"risk_levels":[],"properties":[],"threat_categories":[],"catalog_schemes":[{{"key":"s","label":"S","syntax":{{"prefix":"","required":1,"segments":[{{"min":1,"max":9{digits}}}]}}}}]}}"#
            ));
            MethodologyDef::read_pack(text.as_bytes())
                .ok()
                .and_then(|d| d.catalog_schemes.first().map(|s| s.syntax.segments.clone()))
        };
        let none = Some(vec![SegmentDef {
            min: 1,
            max: 9,
            digits: None,
        }]);
        assert_eq!(seg(""), none);
        assert_eq!(seg(r#","digits":null"#), none);
        assert_eq!(
            seg(r#","digits":2"#),
            Some(vec![SegmentDef {
                min: 1,
                max: 9,
                digits: Some(2)
            }])
        );
    }

    #[test]
    fn write_pack_reports_io_errors() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk full"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("disk full"))
            }
        }
        let def = crate::testkit::second_def();
        assert!(matches!(def.write_pack(Broken), Err(PackError::Io(_))));
    }

    #[test]
    fn read_pack_reports_io_errors() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk gone"))
            }
        }
        assert!(matches!(
            MethodologyDef::read_pack(Broken),
            Err(PackError::Io(_))
        ));
    }
}
