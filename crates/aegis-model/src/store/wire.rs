//! Private serde types for the v1 formats.
//!
//! Reading decodes into owned `*Wire` types whose leaves are bounded:
//! [`BoundedStr`] and [`BoundedVec`] check their cap inside the serde
//! visitor, [`KeySet`] and [`KeyMap`] also reject repeated keys, and
//! [`WireId`] parses the ID text in place. Keys are turned into indices
//! afterwards by the `resolve_*` functions, against the caller's
//! methodology. Writing serializes borrowed `*View` types straight from the
//! project or the op, resolving indices to keys as it goes; nothing is
//! copied first.
//!
//! A cap, a repeated key, a bad ID, or an unknown format found inside a
//! visitor must come out as its own [`StoreError`] variant, but serde only
//! carries a text message. The visitor therefore records a [`Fault`] in a
//! thread-local side channel before it returns the serde error, and
//! [`decode`] reads it back. The channel is cleared before each decode and
//! read once after it, on the same thread, so a fault can never leak from
//! one decode into another.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, Serialize, SerializeMap, SerializeStruct, Serializer};

use super::{Limit, VocabKind};
use crate::asset::PropertySet;
use crate::error::StoreError;
use crate::id::{AssetId, DamageId, ThreatId};
use crate::methodology::{
    CatalogRef, CatalogScheme, ImpactCategory, Level, MAX_IMPACT_CATEGORIES, MAX_PROPERTIES,
    Methodology, Property, ThreatCategory,
};

/// The version both formats are written with and the only one read.
pub(super) const FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Fault side channel.
// ---------------------------------------------------------------------------

/// A typed decode failure found inside a serde visitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Limit(Limit),
    Duplicate,
    InvalidId,
    UnsupportedFormat,
}

impl From<Fault> for StoreError {
    fn from(f: Fault) -> Self {
        match f {
            Fault::Limit(l) => StoreError::LimitExceeded(l),
            Fault::Duplicate => StoreError::DuplicateEntry,
            Fault::InvalidId => StoreError::InvalidId,
            Fault::UnsupportedFormat => StoreError::UnsupportedFormat,
        }
    }
}

thread_local! {
    static FAULT: Cell<Option<Fault>> = const { Cell::new(None) };
}

/// Records `fault` and returns the serde error that stops decoding. The
/// message is fixed text, never input. Time O(1).
fn raise<E: de::Error>(fault: Fault) -> E {
    FAULT.with(|c| c.set(Some(fault)));
    E::custom("aegis store fault")
}

/// Runs one serde decode with a clean side channel and maps its error: a
/// recorded [`Fault`] wins; any other error becomes
/// [`StoreError::Decode`] with its position only. `line`, if given,
/// replaces the parser's line (an op log decodes one line at a time).
/// Time O(1) besides `f`.
pub(super) fn decode<T>(
    line: Option<u64>,
    f: impl FnOnce() -> Result<T, serde_json::Error>,
) -> Result<T, StoreError> {
    FAULT.with(|c| c.set(None));
    let result = f();
    let fault = FAULT.with(Cell::take);
    result.map_err(|e| match fault {
        Some(fault) => fault.into(),
        None => StoreError::Decode {
            line: line.unwrap_or_else(|| to_u64(e.line())),
            column: to_u64(e.column()),
        },
    })
}

/// Widens a `usize` (always fits on supported targets; saturates otherwise).
pub(super) fn to_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// Decodes one complete JSON value from `bytes`; trailing data other than
/// whitespace is an error. Time O(bytes).
pub(super) fn from_slice<'a, T: Deserialize<'a>>(
    bytes: &'a [u8],
    line: Option<u64>,
) -> Result<T, StoreError> {
    decode(line, || {
        let mut de = serde_json::Deserializer::from_slice(bytes);
        let value = T::deserialize(&mut de)?;
        de.end()?;
        Ok(value)
    })
}

/// Maps a serialization error: I/O stays I/O; anything else is a
/// [`Required`] or check failure in a view, so the data is unencodable.
pub(super) fn encode_error(e: serde_json::Error) -> StoreError {
    if e.is_io() {
        StoreError::Io(e.into())
    } else {
        StoreError::UnencodableOp
    }
}

// ---------------------------------------------------------------------------
// Bounded leaves.
// ---------------------------------------------------------------------------

/// A decode cap: its size and the [`Limit`] reported when it breaks.
pub(super) trait Cap {
    const MAX: usize;
    const LIMIT: Limit;
}

macro_rules! caps {
    ($($name:ident = $max:expr, $limit:expr;)*) => {$(
        pub(super) enum $name {}
        impl Cap for $name {
            const MAX: usize = $max;
            const LIMIT: Limit = $limit;
        }
    )*};
}

caps! {
    NameCap = super::MAX_NAME_BYTES, Limit::NAME;
    KeyCap = super::MAX_KEY_BYTES, Limit::KEY;
    VersionCap = super::MAX_METHODOLOGY_VERSION_BYTES, Limit::VERSION;
    RefTextCap = super::MAX_CATALOG_REF_BYTES, Limit::CATALOG_REF;
    RefsCap = crate::threat::MAX_CATALOG_REFS, Limit::CATALOG_REFS;
    LinksCap = crate::threat::MAX_DAMAGE_LINKS, Limit::DAMAGE_LINKS;
}

/// A string of at most `C::MAX` bytes. The check runs in the visitor, on
/// the parser's borrowed text, before anything is allocated.
pub(super) struct BoundedStr<C>(pub(super) Arc<str>, PhantomData<C>);

pub(super) type Name = BoundedStr<NameCap>;
pub(super) type Key = BoundedStr<KeyCap>;
pub(super) type VersionText = BoundedStr<VersionCap>;
pub(super) type RefText = BoundedStr<RefTextCap>;

impl<'de, C: Cap> Deserialize<'de> for BoundedStr<C> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<C>(PhantomData<C>);
        impl<C: Cap> Visitor<'_> for V<C> {
            type Value = BoundedStr<C>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                if s.len() > C::MAX {
                    return Err(raise(Fault::Limit(C::LIMIT)));
                }
                Ok(BoundedStr(Arc::from(s), PhantomData))
            }
        }
        d.deserialize_str(V(PhantomData))
    }
}

/// A list of at most `C::MAX` items. Pre-allocates at most
/// `min(size_hint, C::MAX)` and stops on item `C::MAX + 1`, so it never
/// holds more than `C::MAX` items.
pub(super) struct BoundedVec<T, C>(pub(super) Vec<T>, PhantomData<C>);

impl<'de, T: Deserialize<'de>, C: Cap> Deserialize<'de> for BoundedVec<T, C> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, C>(PhantomData<(T, C)>);
        impl<'de, T: Deserialize<'de>, C: Cap> Visitor<'de> for V<T, C> {
            type Value = BoundedVec<T, C>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(C::MAX));
                while let Some(item) = seq.next_element()? {
                    if items.len() >= C::MAX {
                        return Err(raise(Fault::Limit(C::LIMIT)));
                    }
                    items.push(item);
                }
                Ok(BoundedVec(items, PhantomData))
            }
        }
        d.deserialize_seq(V(PhantomData))
    }
}

/// A set of keys: at most 64 (the most properties a methodology declares),
/// none repeated. Held sorted, since a set has no order. Time O(k log k).
pub(super) struct KeySet(pub(super) Vec<Key>);

impl<'de> Deserialize<'de> for KeySet {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = KeySet;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of keys")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<KeySet, A::Error> {
                let mut keys: Vec<Key> =
                    Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_PROPERTIES));
                while let Some(key) = seq.next_element::<Key>()? {
                    if keys.len() >= MAX_PROPERTIES {
                        return Err(raise(Fault::Limit(Limit::PROPERTIES)));
                    }
                    match keys.binary_search_by(|k| k.0.cmp(&key.0)) {
                        Ok(_) => return Err(raise(Fault::Duplicate)),
                        Err(pos) => keys.insert(pos, key),
                    }
                }
                Ok(KeySet(keys))
            }
        }
        d.deserialize_seq(V)
    }
}

/// A map from impact category key to level key, in input order: at most 32
/// entries (the most categories a methodology declares), no category
/// repeated. Time O(c log c).
pub(super) struct KeyMap(pub(super) Vec<(Key, Key)>);

impl<'de> Deserialize<'de> for KeyMap {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = KeyMap;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of keys")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KeyMap, A::Error> {
                let cap = map.size_hint().unwrap_or(0).min(MAX_IMPACT_CATEGORIES);
                let mut pairs: Vec<(Key, Key)> = Vec::with_capacity(cap);
                let mut seen: Vec<Arc<str>> = Vec::with_capacity(cap);
                while let Some(key) = map.next_key::<Key>()? {
                    if pairs.len() >= MAX_IMPACT_CATEGORIES {
                        return Err(raise(Fault::Limit(Limit::IMPACTS)));
                    }
                    match seen.binary_search(&key.0) {
                        Ok(_) => return Err(raise(Fault::Duplicate)),
                        Err(pos) => seen.insert(pos, Arc::clone(&key.0)),
                    }
                    let value = map.next_value::<Key>()?;
                    pairs.push((key, value));
                }
                Ok(KeyMap(pairs))
            }
        }
        d.deserialize_map(V)
    }
}

/// A public ID decoded from its canonical text: 36 bytes, hyphenated,
/// lowercase hex, then checked as a `UUIDv7` by the ID's `FromStr`.
/// Anything else is [`StoreError::InvalidId`]. Time O(1).
pub(super) struct WireId<T>(pub(super) T);

/// Checks the canonical `8-4-4-4-12` lowercase form. Time O(1).
fn is_canonical_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

impl<'de, T: FromStr> Deserialize<'de> for WireId<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<T: FromStr> Visitor<'_> for V<T> {
            type Value = WireId<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a UUIDv7 string")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                if !is_canonical_uuid(s) {
                    return Err(raise(Fault::InvalidId));
                }
                T::from_str(s)
                    .map(WireId)
                    .map_err(|_| raise(Fault::InvalidId))
            }
        }
        d.deserialize_str(V(PhantomData))
    }
}

/// A format name, `aegis-project` or `aegis-oplog`.
pub(super) trait FormatName {
    const NAME: &'static str;
}

/// The snapshot format.
pub(super) enum ProjectFormat {}
impl FormatName for ProjectFormat {
    const NAME: &'static str = "aegis-project";
}

/// The op log format.
pub(super) enum OpLogFormat {}
impl FormatName for OpLogFormat {
    const NAME: &'static str = "aegis-oplog";
}

/// The `format` field: exactly `F::NAME`, else
/// [`StoreError::UnsupportedFormat`] as soon as it is read.
pub(super) struct Format<F>(PhantomData<F>);

impl<'de, F: FormatName> Deserialize<'de> for Format<F> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<F>(PhantomData<F>);
        impl<F: FormatName> Visitor<'_> for V<F> {
            type Value = Format<F>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a format name")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<Self::Value, E> {
                if s == F::NAME {
                    Ok(Format(PhantomData))
                } else {
                    Err(raise(Fault::UnsupportedFormat))
                }
            }
        }
        d.deserialize_str(V(PhantomData))
    }
}

/// The format `version` field: exactly [`FORMAT_VERSION`]; any other
/// number is [`StoreError::UnsupportedFormat`] as soon as it is read.
pub(super) struct FormatVersion;

impl<'de> Deserialize<'de> for FormatVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl Visitor<'_> for V {
            type Value = FormatVersion;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a format version number")
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<FormatVersion, E> {
                if v == u64::from(FORMAT_VERSION) {
                    Ok(FormatVersion)
                } else {
                    Err(raise(Fault::UnsupportedFormat))
                }
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<FormatVersion, E> {
                Err(raise(Fault::UnsupportedFormat))
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<FormatVersion, E> {
                Err(raise(Fault::UnsupportedFormat))
            }
        }
        d.deserialize_u64(V)
    }
}

// ---------------------------------------------------------------------------
// Owned wire types (reading).
// ---------------------------------------------------------------------------

/// `{"key": ..., "version": ...}`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MethodologyWire {
    pub(super) key: Key,
    pub(super) version: VersionText,
}

/// `{"scheme": <key>, "id": <text>}`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CatalogRefWire {
    pub(super) scheme: Key,
    pub(super) id: RefText,
}

/// A snapshot.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapshotWire {
    // Checked while decoding, then not needed.
    #[serde(rename = "format")]
    _format: Format<ProjectFormat>,
    #[serde(rename = "version")]
    _version: FormatVersion,
    pub(super) methodology: MethodologyWire,
    pub(super) assets: Vec<AssetWire>,
    pub(super) damage_scenarios: Vec<DamageWire>,
    pub(super) threat_scenarios: Vec<ThreatWire>,
}

/// A snapshot asset.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssetWire {
    pub(super) id: WireId<AssetId>,
    pub(super) name: Name,
    pub(super) properties: KeySet,
}

/// A snapshot damage scenario; unrated categories are absent.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageWire {
    pub(super) id: WireId<DamageId>,
    pub(super) name: Name,
    pub(super) impacts: KeyMap,
}

/// A snapshot threat scenario.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThreatWire {
    pub(super) id: WireId<ThreatId>,
    pub(super) name: Name,
    pub(super) asset: WireId<AssetId>,
    pub(super) category: Key,
    pub(super) catalog_refs: BoundedVec<CatalogRefWire, RefsCap>,
    pub(super) damages: BoundedVec<WireId<DamageId>, LinksCap>,
}

/// The op log header line.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeaderWire {
    // Checked while decoding, then not needed.
    #[serde(rename = "format")]
    _format: Format<OpLogFormat>,
    #[serde(rename = "version")]
    _version: FormatVersion,
    pub(super) methodology: MethodologyWire,
}

/// One op log record line.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordWire {
    pub(super) seq: u64,
    pub(super) op: OpWire,
}

/// An op, internally tagged by `"op"`. Field names are the `Op` field
/// names.
#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OpWire {
    AddAsset {
        id: WireId<AssetId>,
        name: Name,
        properties: KeySet,
    },
    RenameAsset {
        id: WireId<AssetId>,
        name: Name,
    },
    SetAssetProperties {
        id: WireId<AssetId>,
        properties: KeySet,
    },
    RemoveAsset {
        id: WireId<AssetId>,
    },
    AddDamageScenario {
        id: WireId<DamageId>,
        name: Name,
        impacts: KeyMap,
    },
    RenameDamageScenario {
        id: WireId<DamageId>,
        name: Name,
    },
    SetImpact {
        id: WireId<DamageId>,
        category: Key,
        // `deserialize_with` makes the field required: `null` is unrated,
        // a missing field is an error.
        #[serde(deserialize_with = "Option::deserialize")]
        level: Option<Key>,
    },
    RemoveDamageScenario {
        id: WireId<DamageId>,
    },
    AddThreatScenario {
        id: WireId<ThreatId>,
        name: Name,
        asset: WireId<AssetId>,
        category: Key,
        catalog_refs: BoundedVec<CatalogRefWire, RefsCap>,
        damages: BoundedVec<WireId<DamageId>, LinksCap>,
    },
    RenameThreatScenario {
        id: WireId<ThreatId>,
        name: Name,
    },
    SetThreatCategory {
        id: WireId<ThreatId>,
        category: Key,
    },
    AddCatalogRef {
        threat: WireId<ThreatId>,
        catalog_ref: CatalogRefWire,
    },
    RemoveCatalogRef {
        threat: WireId<ThreatId>,
        catalog_ref: CatalogRefWire,
    },
    LinkDamage {
        threat: WireId<ThreatId>,
        damage: WireId<DamageId>,
    },
    UnlinkDamage {
        threat: WireId<ThreatId>,
        damage: WireId<DamageId>,
    },
    RemoveThreatScenario {
        id: WireId<ThreatId>,
    },
}

// ---------------------------------------------------------------------------
// Keys to indices.
// ---------------------------------------------------------------------------

fn unknown(kind: VocabKind, key: &Key) -> StoreError {
    StoreError::UnknownKey {
        kind,
        key: Arc::clone(&key.0),
    }
}

/// Checks the file's methodology key against the supplied one; any version
/// is accepted (ADR 0009, open question 1). Time O(key length).
pub(super) fn check_methodology(w: &MethodologyWire, m: &Methodology) -> Result<(), StoreError> {
    if &*w.key.0 == m.key() {
        Ok(())
    } else {
        Err(StoreError::MethodologyMismatch {
            expected: Arc::from(m.key()),
            found: Arc::clone(&w.key.0),
        })
    }
}

/// Resolves property keys. Time O(k log m).
pub(super) fn resolve_properties(
    m: &Methodology,
    keys: &KeySet,
) -> Result<PropertySet, StoreError> {
    keys.0.iter().try_fold(PropertySet::empty(), |set, k| {
        m.property(&k.0)
            .map(|p| set.with(p))
            .ok_or_else(|| unknown(VocabKind::Property, k))
    })
}

/// Resolves an impact category key. Time O(log m).
pub(super) fn resolve_impact_category(
    m: &Methodology,
    key: &Key,
) -> Result<ImpactCategory, StoreError> {
    m.impact_category(&key.0)
        .ok_or_else(|| unknown(VocabKind::ImpactCategory, key))
}

/// Resolves a level key within `cat`. Time O(log m).
pub(super) fn resolve_level(
    m: &Methodology,
    cat: ImpactCategory,
    key: &Key,
) -> Result<Level, StoreError> {
    m.level(cat, &key.0)
        .ok_or_else(|| unknown(VocabKind::Level, key))
}

/// Resolves impact ratings, keeping their order. Time O(c log m).
pub(super) fn resolve_impacts(
    m: &Methodology,
    map: &KeyMap,
) -> Result<Vec<(ImpactCategory, Level)>, StoreError> {
    map.0
        .iter()
        .map(|(c, l)| {
            let cat = resolve_impact_category(m, c)?;
            Ok((cat, resolve_level(m, cat, l)?))
        })
        .collect()
}

/// Resolves a threat category key. Time O(log m).
pub(super) fn resolve_threat_category(
    m: &Methodology,
    key: &Key,
) -> Result<ThreatCategory, StoreError> {
    m.threat_category(&key.0)
        .ok_or_else(|| unknown(VocabKind::ThreatCategory, key))
}

/// Resolves a catalog ref: the scheme key, then the text parsed again
/// under the scheme's syntax. Time O(log m + text length).
pub(super) fn resolve_catalog_ref(
    m: &Methodology,
    w: &CatalogRefWire,
) -> Result<CatalogRef, StoreError> {
    let scheme = m
        .catalog_scheme(&w.scheme.0)
        .ok_or_else(|| unknown(VocabKind::CatalogScheme, &w.scheme))?;
    m.parse_catalog_ref(scheme, &w.id.0)
        .map_err(|_| StoreError::InvalidCatalogRef {
            scheme: Arc::clone(&w.scheme.0),
        })
}

/// Resolves a list of catalog refs, keeping their order. Time O(k (log m +
/// text length)).
pub(super) fn resolve_catalog_refs(
    m: &Methodology,
    refs: &[CatalogRefWire],
) -> Result<Vec<CatalogRef>, StoreError> {
    refs.iter().map(|r| resolve_catalog_ref(m, r)).collect()
}

// ---------------------------------------------------------------------------
// Key order (writing).
// ---------------------------------------------------------------------------

/// A methodology's vocabularies in key order, so output order never
/// depends on declaration order (P4). Built once per save or per writer in
/// O(m log m); space O(m).
#[derive(Clone, Debug)]
pub(super) struct KeyOrder {
    /// Rank in key order of each property, by property index.
    property_rank: Vec<u8>,
    /// Properties sorted by key.
    properties: Vec<Property>,
    /// Impact categories sorted by key.
    impact_categories: Vec<ImpactCategory>,
    /// Catalog schemes sorted by key.
    catalog_schemes: Vec<CatalogScheme>,
}

impl KeyOrder {
    pub(super) fn new(m: &Methodology) -> Self {
        let mut properties: Vec<Property> = m.properties().collect();
        properties.sort_by_key(|&p| m.property_key(p));
        let mut property_rank = vec![u8::MAX; properties.len()];
        for (rank, p) in properties.iter().enumerate() {
            if let (Some(slot), Ok(rank)) = (property_rank.get_mut(p.index()), u8::try_from(rank)) {
                *slot = rank;
            }
        }
        let mut impact_categories: Vec<ImpactCategory> = m.impact_categories().collect();
        impact_categories.sort_by_key(|&c| m.impact_category_key(c));
        let mut catalog_schemes: Vec<CatalogScheme> = m.catalog_schemes().collect();
        catalog_schemes.sort_by_key(|&s| m.catalog_scheme_key(s));
        KeyOrder {
            property_rank,
            properties,
            impact_categories,
            catalog_schemes,
        }
    }

    /// The members of `set` in key order, or `None` if one is not in the
    /// methodology. Permutes the set's bits by rank, then walks them:
    /// O(p), no allocation.
    fn sorted_properties(&self, set: PropertySet) -> Option<impl Iterator<Item = Property> + '_> {
        let mut ranked = 0u64;
        for p in set.iter() {
            let rank = *self.property_rank.get(p.index())?;
            ranked |= 1u64.checked_shl(u32::from(rank))?;
        }
        let mut rest = ranked;
        Some(std::iter::from_fn(move || {
            if rest == 0 {
                return None;
            }
            let rank = rest.trailing_zeros();
            rest &= rest - 1;
            usize::try_from(rank)
                .ok()
                .and_then(|r| self.properties.get(r))
                .copied()
        }))
    }
}

// ---------------------------------------------------------------------------
// Borrowed views (writing).
// ---------------------------------------------------------------------------

/// A value that must be present; `None` fails serialization, which the
/// caller maps to [`StoreError::UnencodableOp`].
pub(super) struct Required<T>(pub(super) Option<T>);

impl<T: Serialize> Serialize for Required<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match &self.0 {
            Some(v) => v.serialize(s),
            None => Err(ser::Error::custom("value not in the methodology")),
        }
    }
}

/// Writes a `Display` value as a JSON string without an intermediate
/// `String`.
pub(super) struct Text<D>(pub(super) D);

impl<D: fmt::Display> Serialize for Text<D> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.0)
    }
}

/// `{"key": ..., "version": ...}`.
#[derive(serde::Serialize)]
pub(super) struct MethodologyView<'a> {
    pub(super) key: &'a str,
    pub(super) version: &'a str,
}

impl<'a> MethodologyView<'a> {
    pub(super) fn of(m: &'a Methodology) -> Self {
        MethodologyView {
            key: m.key(),
            version: m.version(),
        }
    }
}

/// The op log header.
#[derive(serde::Serialize)]
pub(super) struct HeaderView<'a> {
    pub(super) format: &'static str,
    pub(super) version: u32,
    pub(super) methodology: MethodologyView<'a>,
}

/// A property set as keys in key order. O(p).
pub(super) struct PropertyKeys<'a> {
    pub(super) m: &'a Methodology,
    pub(super) order: &'a KeyOrder,
    pub(super) set: PropertySet,
}

impl Serialize for PropertyKeys<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let sorted = self
            .order
            .sorted_properties(self.set)
            .ok_or_else(|| ser::Error::custom("property not in the methodology"))?;
        s.collect_seq(sorted.map(|p| Required(self.m.property_key(p))))
    }
}

/// One catalog ref as `{"scheme": <key>, "id": <text>}`. Fails unless the
/// ref satisfies its scheme's syntax, so the text always parses back to the
/// same ref. O(1).
pub(super) struct CatalogRefView<'a> {
    pub(super) m: &'a Methodology,
    pub(super) r: CatalogRef,
}

impl Serialize for CatalogRefView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let scheme = self
            .m
            .catalog_scheme_key(self.r.scheme())
            .filter(|_| self.m.check_catalog_ref(&self.r).is_ok())
            .ok_or_else(|| ser::Error::custom("catalog ref not in the methodology"))?;
        let mut st = s.serialize_struct("CatalogRef", 2)?;
        st.serialize_field("scheme", scheme)?;
        st.serialize_field("id", &Text(self.m.display_catalog_ref(&self.r)))?;
        st.end()
    }
}

/// Catalog refs in the given order (ops). O(k).
pub(super) struct CatalogRefsInOrder<'a> {
    pub(super) m: &'a Methodology,
    pub(super) refs: &'a [CatalogRef],
}

impl Serialize for CatalogRefsInOrder<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.refs.iter().map(|&r| CatalogRefView { m: self.m, r }))
    }
}

/// A threat's stored refs (sorted by scheme index, then segments) in
/// canonical order: by scheme key, then segments. Each scheme's refs are a
/// contiguous run, found by binary search: O(S log k + k), no allocation.
pub(super) struct CatalogRefsByKey<'a> {
    pub(super) m: &'a Methodology,
    pub(super) order: &'a KeyOrder,
    pub(super) refs: &'a [CatalogRef],
}

impl Serialize for CatalogRefsByKey<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let refs = self.refs;
        let run = |scheme: CatalogScheme| {
            let start = refs.partition_point(|r| r.scheme() < scheme);
            let end = refs.partition_point(|r| r.scheme() <= scheme);
            refs.get(start..end).unwrap_or(&[])
        };
        // Refs under a scheme the methodology lacks are in no run; count so
        // they fail instead of vanishing.
        let covered: usize = self
            .order
            .catalog_schemes
            .iter()
            .map(|&scheme| run(scheme).len())
            .sum();
        if covered != refs.len() {
            return Err(ser::Error::custom("catalog ref not in the methodology"));
        }
        let runs = self
            .order
            .catalog_schemes
            .iter()
            .flat_map(|&scheme| run(scheme));
        s.collect_seq(runs.map(|&r| CatalogRefView { m: self.m, r }))
    }
}

/// Impact ratings in the given order (ops), as an object. Fails on a
/// repeated category, which could not be read back. O(c).
pub(super) struct ImpactsInOrder<'a> {
    pub(super) m: &'a Methodology,
    pub(super) impacts: &'a [(ImpactCategory, Level)],
}

impl Serialize for ImpactsInOrder<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.impacts.len()))?;
        let mut seen = 0u64;
        for &(c, l) in self.impacts {
            let bit = u32::try_from(c.index())
                .ok()
                .and_then(|i| 1u64.checked_shl(i))
                .unwrap_or(0);
            let (Some(ck), Some(lk)) = (self.m.impact_category_key(c), self.m.level_key(c, l))
            else {
                return Err(ser::Error::custom("rating not in the methodology"));
            };
            if bit == 0 || seen & bit != 0 {
                return Err(ser::Error::custom("impact category repeated"));
            }
            seen |= bit;
            map.serialize_entry(ck, lk)?;
        }
        map.end()
    }
}

/// A stored damage scenario's ratings by category key, unrated ones left
/// out, as an object. O(c).
pub(super) struct ImpactsByKey<'a> {
    pub(super) m: &'a Methodology,
    pub(super) order: &'a KeyOrder,
    pub(super) damage: &'a crate::damage::DamageScenario,
}

impl Serialize for ImpactsByKey<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        for &c in &self.order.impact_categories {
            if let Some(l) = self.damage.impact(c) {
                map.serialize_entry(
                    &Required(self.m.impact_category_key(c)),
                    &Required(self.m.level_key(c, l)),
                )?;
            }
        }
        map.end()
    }
}

/// A list of public IDs. O(k).
pub(super) struct Ids<'a, T>(pub(super) &'a [T]);

impl<T: fmt::Display> Serialize for Ids<'_, T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(Text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    fn de<T: for<'a> Deserialize<'a>>(json: &str) -> Result<T, StoreError> {
        from_slice(json.as_bytes(), Some(7))
    }

    #[test]
    fn fault_channel_maps_and_clears() {
        let long = format!("\"{}\"", "k".repeat(65));
        let err = de::<Key>(&long).err();
        assert!(matches!(
            err,
            Some(StoreError::LimitExceeded(Limit::KeyBytes { max: 64 }))
        ));
        // The next decode starts clean: a plain shape error is `Decode`.
        let err = de::<Key>("12").err();
        assert!(matches!(err, Some(StoreError::Decode { line: 7, .. })));
        assert!(de::<Key>("\"ok\"").is_ok());
    }

    #[test]
    fn bounded_str_cap_is_inclusive() {
        let at = format!("\"{}\"", "k".repeat(64));
        assert_eq!(de::<Key>(&at).map(|k| k.0.len()).ok(), Some(64));
    }

    #[test]
    fn bounded_vec_stops_at_cap() {
        type Refs = BoundedVec<Key, RefsCap>;
        let ok = format!("[{}]", vec!["\"x\""; 16].join(","));
        let over = format!("[{}]", vec!["\"x\""; 17].join(","));
        assert_eq!(de::<Refs>(&ok).map(|v| v.0.len()).ok(), Some(16));
        assert!(matches!(
            de::<Refs>(&over).err(),
            Some(StoreError::LimitExceeded(Limit::CatalogRefs { max: 16 }))
        ));
    }

    #[test]
    fn key_set_sorts_and_rejects_repeats() {
        let set = de::<KeySet>(r#"["b","a","c"]"#).ok();
        let keys: Option<Vec<String>> = set.map(|s| s.0.iter().map(|k| k.0.to_string()).collect());
        assert_eq!(keys, Some(vec!["a".into(), "b".into(), "c".into()]));
        assert!(matches!(
            de::<KeySet>(r#"["a","b","a"]"#).err(),
            Some(StoreError::DuplicateEntry)
        ));
    }

    #[test]
    fn key_map_keeps_order_and_rejects_repeats() {
        let map = de::<KeyMap>(r#"{"z":"1","a":"2"}"#).ok();
        let keys: Option<Vec<String>> =
            map.map(|m| m.0.iter().map(|(k, _)| k.0.to_string()).collect());
        assert_eq!(keys, Some(vec!["z".into(), "a".into()]));
        assert!(matches!(
            de::<KeyMap>(r#"{"a":"1","a":"2"}"#).err(),
            Some(StoreError::DuplicateEntry)
        ));
    }

    #[test]
    fn wire_id_accepts_only_canonical_v7() {
        let id = testkit::asset_id(3);
        let ok = format!("\"{id}\"");
        assert_eq!(de::<WireId<AssetId>>(&ok).map(|w| w.0).ok(), Some(id));
        let upper = ok.to_uppercase();
        let simple = ok.replace('-', "");
        for bad in [
            upper.as_str(),
            simple.as_str(),
            "\"67e55044-10b1-426f-9247-bb680e5fe0c8\"",
            "\"not an id\"",
        ] {
            assert!(
                matches!(
                    de::<WireId<AssetId>>(bad).err(),
                    Some(StoreError::InvalidId)
                ),
                "{bad}"
            );
        }
        assert!(is_canonical_uuid("0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"));
        assert!(!is_canonical_uuid("0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5"));
    }

    #[test]
    fn format_and_version_checks() {
        assert!(de::<Format<ProjectFormat>>("\"aegis-project\"").is_ok());
        for bad in ["\"aegis-oplog\"", "\"x\""] {
            assert!(matches!(
                de::<Format<ProjectFormat>>(bad).err(),
                Some(StoreError::UnsupportedFormat)
            ));
        }
        assert!(de::<FormatVersion>("1").is_ok());
        for bad in ["2", "0", "-1", "1.5"] {
            assert!(matches!(
                de::<FormatVersion>(bad).err(),
                Some(StoreError::UnsupportedFormat)
            ));
        }
        assert!(matches!(
            de::<FormatVersion>("\"1\"").err(),
            Some(StoreError::Decode { .. })
        ));
    }

    #[test]
    fn key_order_sorts_by_key() {
        let m = testkit::second();
        let order = KeyOrder::new(&m);
        let keys: Vec<_> = order
            .sorted_properties(m.all_properties())
            .map(|it| it.filter_map(|p| m.property_key(p)).collect())
            .unwrap_or_default();
        assert_eq!(keys, ["a", "c", "i"]);
        let cats: Vec<_> = order
            .impact_categories
            .iter()
            .filter_map(|&c| m.impact_category_key(c))
            .collect();
        assert_eq!(cats, ["data-exposure", "patient-harm", "service-loss"]);
        let wide = testkit::wide();
        let foreign = wide.all_properties();
        assert!(order.sorted_properties(foreign).is_none());
    }

    #[test]
    fn resolve_reports_the_vocabulary() {
        let m = testkit::builtin();
        let key = |s: &str| BoundedStr::<KeyCap>(Arc::from(s), PhantomData);
        assert!(matches!(
            resolve_threat_category(&m, &key("nope")),
            Err(StoreError::UnknownKey {
                kind: VocabKind::ThreatCategory,
                ..
            })
        ));
        let r = CatalogRefWire {
            scheme: key("nope"),
            id: BoundedStr(Arc::from("1"), PhantomData),
        };
        assert!(matches!(
            resolve_catalog_ref(&m, &r),
            Err(StoreError::UnknownKey {
                kind: VocabKind::CatalogScheme,
                ..
            })
        ));
    }
}
