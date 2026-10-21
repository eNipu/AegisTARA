//! Private serde types for the v1 formats.
//!
//! Reading decodes into owned `*Wire` types whose leaves are bounded and
//! already resolved: [`BoundedStr`] and [`BoundedVec`] check their cap
//! inside the serde visitor; [`WireId`] parses the ID text in place; and
//! every vocabulary key is looked up in the caller's methodology the moment
//! it is read ([`PropertySetKeys`], [`Impacts`], [`ThreatCategoryKey`],
//! [`ImpactCategoryKey`], [`CatalogRefText`]), so a decoded entity holds
//! 1-byte indices, never key strings. The only strings kept are names (and,
//! once per op log record, a `SetImpact` level key, which needs its
//! category first). Writing serializes borrowed `*View` types straight from
//! the project or the op, resolving indices to keys as it goes; nothing is
//! copied first.
//!
//! Two thread-local cells connect the visitors with [`decode`], which is
//! the only entry point:
//!
//! - **Context.** `decode` installs the methodology for the duration of one
//!   decode (an RAII guard restores the previous value, also on unwind).
//!   This is the `DeserializeSeed` idea without threading a seed through
//!   every derived struct: derive keeps its strict field handling
//!   (`deny_unknown_fields`, duplicate and missing fields) for all the wire
//!   shapes, including the internally tagged ops.
//! - **Faults.** A cap, a repeated key, a bad ID, an unknown key, a bad
//!   catalog ref, a wrong format, or another methodology found inside a
//!   visitor must come out as its own [`StoreError`] variant, but serde only
//!   carries a text message. The visitor records a [`Fault`] before it
//!   returns a fixed-text serde error, and `decode` reads it back. The cell
//!   is cleared before each decode and taken once after it, on the same
//!   thread, so a fault can never leak from one decode into another.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, Serialize, SerializeMap, SerializeStruct, Serializer};

use super::{Limit, MAX_KEY_BYTES, VocabKind};
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
// Decode context and fault side channel.
// ---------------------------------------------------------------------------

/// A typed decode failure found inside a serde visitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Fault {
    Limit(Limit),
    Duplicate,
    InvalidId,
    /// The format name or version is not valid (`None`), or the format
    /// version is the given number, above the current one.
    UnsupportedFormat(Option<u64>),
    MethodologyMismatch {
        expected: Arc<str>,
        found: Arc<str>,
    },
    UnknownKey {
        kind: VocabKind,
        key: Arc<str>,
    },
    InvalidCatalogRef {
        scheme: Arc<str>,
    },
    /// A visitor ran outside [`decode`]. Never happens: `decode` is the only
    /// entry point and always installs the methodology.
    NoContext,
}

impl From<Fault> for StoreError {
    fn from(f: Fault) -> Self {
        match f {
            Fault::Limit(l) => StoreError::LimitExceeded(l),
            Fault::Duplicate => StoreError::DuplicateEntry,
            Fault::InvalidId => StoreError::InvalidId,
            Fault::UnsupportedFormat(version) => StoreError::UnsupportedFormat { version },
            Fault::MethodologyMismatch { expected, found } => {
                StoreError::MethodologyMismatch { expected, found }
            }
            Fault::UnknownKey { kind, key } => StoreError::UnknownKey { kind, key },
            Fault::InvalidCatalogRef { scheme } => StoreError::InvalidCatalogRef { scheme },
            Fault::NoContext => StoreError::Decode { line: 0, column: 0 },
        }
    }
}

thread_local! {
    static FAULT: Cell<Option<Fault>> = const { Cell::new(None) };
    static CONTEXT: RefCell<Option<Arc<Methodology>>> = const { RefCell::new(None) };
}

/// Records `fault` and returns the serde error that stops decoding. The
/// message is fixed text, never input. Time O(1).
fn raise<E: de::Error>(fault: Fault) -> E {
    FAULT.with(|c| c.set(Some(fault)));
    E::custom("aegis store fault")
}

/// Installs a methodology as the decode context; restores the previous one
/// when dropped.
struct ContextGuard {
    previous: Option<Arc<Methodology>>,
}

impl ContextGuard {
    fn enter(m: &Arc<Methodology>) -> Self {
        let previous = CONTEXT.with(|c| {
            c.try_borrow_mut()
                .ok()
                .and_then(|mut slot| slot.replace(Arc::clone(m)))
        });
        ContextGuard { previous }
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        let previous = self.previous.take();
        CONTEXT.with(|c| {
            if let Ok(mut slot) = c.try_borrow_mut() {
                *slot = previous;
            }
        });
    }
}

/// The methodology of the running decode. Time O(1) (one reference count).
fn context() -> Result<Arc<Methodology>, Fault> {
    CONTEXT
        .with(|c| c.try_borrow().ok().and_then(|slot| slot.clone()))
        .ok_or(Fault::NoContext)
}

/// Runs one serde decode against `m` with a clean side channel and maps its
/// error: a recorded [`Fault`] wins; an I/O error stays [`StoreError::Io`];
/// any other error becomes [`StoreError::Decode`] with its position only.
/// `line`, if given, replaces the parser's line (an op log decodes one line
/// at a time). Time O(1) besides `f`.
pub(super) fn decode<T>(
    m: &Arc<Methodology>,
    line: Option<u64>,
    f: impl FnOnce() -> Result<T, serde_json::Error>,
) -> Result<T, StoreError> {
    let guard = ContextGuard::enter(m);
    FAULT.with(|c| c.set(None));
    let result = f();
    let fault = FAULT.with(Cell::take);
    drop(guard);
    result.map_err(|e| match fault {
        Some(fault) => fault.into(),
        None if e.is_io() => StoreError::Io(e.into()),
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

/// Decodes one complete JSON value from `bytes` against `m`; trailing data
/// other than whitespace is an error. Time O(bytes).
pub(super) fn from_slice<'a, T: Deserialize<'a>>(
    m: &Arc<Methodology>,
    bytes: &'a [u8],
    line: Option<u64>,
) -> Result<T, StoreError> {
    decode(m, line, || {
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

/// A string of at most `C::MAX` bytes, kept. The check runs in the visitor,
/// on the parser's text, before anything is allocated.
pub(super) struct BoundedStr<C>(pub(super) Arc<str>, PhantomData<C>);

pub(super) type Name = BoundedStr<NameCap>;
pub(super) type Key = BoundedStr<KeyCap>;
pub(super) type VersionText = BoundedStr<VersionCap>;
pub(super) type RefText = BoundedStr<RefTextCap>;

impl<'de, C: Cap> Deserialize<'de> for BoundedStr<C> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            if s.len() > C::MAX {
                return Err(Fault::Limit(C::LIMIT));
            }
            Ok(BoundedStr(Arc::from(s), PhantomData))
        })
        .deserialize(d)
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

// ---------------------------------------------------------------------------
// Keys resolved while decoding.
// ---------------------------------------------------------------------------

/// A seed that hands the next JSON string to `f` on the parser's own text
/// (borrowed from the input, or from the parser's reused scratch buffer),
/// without allocating. A fault from `f` stops decoding. Time O(1) besides
/// `f`.
struct WithStr<F, T>(F, PhantomData<fn() -> T>);

impl<F, T> WithStr<F, T> {
    fn new(f: F) -> Self {
        WithStr(f, PhantomData)
    }
}

impl<'de, F: FnOnce(&str) -> Result<T, Fault>, T> DeserializeSeed<'de> for WithStr<F, T> {
    type Value = T;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<T, D::Error> {
        struct V<F, T>(F, PhantomData<fn() -> T>);
        impl<F: FnOnce(&str) -> Result<T, Fault>, T> Visitor<'_> for V<F, T> {
            type Value = T;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<T, E> {
                (self.0)(s).map_err(raise)
            }
        }
        d.deserialize_str(V(self.0, PhantomData))
    }
}

/// Checks the key length cap. Time O(1).
fn check_key_len(s: &str) -> Result<(), Fault> {
    if s.len() > MAX_KEY_BYTES {
        Err(Fault::Limit(Limit::KEY))
    } else {
        Ok(())
    }
}

/// The fault for an unknown key; allocates the (at most 64-byte) key once.
fn unknown_key(kind: VocabKind, s: &str) -> Fault {
    Fault::UnknownKey {
        kind,
        key: Arc::from(s),
    }
}

/// Returns `found`, or notes the first unknown key of a list in `pending`
/// (raised when the list ends, so a list's cap is reported first) and
/// returns `None`. Time O(1), one allocation for the first unknown key only.
fn note_unknown<T>(
    found: Option<T>,
    kind: VocabKind,
    s: &str,
    pending: &mut Option<Fault>,
) -> Option<T> {
    if found.is_none() && pending.is_none() {
        *pending = Some(unknown_key(kind, s));
    }
    found
}

/// A threat category key, resolved as it is read.
pub(super) struct ThreatCategoryKey(pub(super) ThreatCategory);

impl<'de> Deserialize<'de> for ThreatCategoryKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .threat_category(s)
                .map(ThreatCategoryKey)
                .ok_or_else(|| unknown_key(VocabKind::ThreatCategory, s))
        })
        .deserialize(d)
    }
}

/// An impact category key, resolved as it is read.
pub(super) struct ImpactCategoryKey(pub(super) ImpactCategory);

impl<'de> Deserialize<'de> for ImpactCategoryKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .impact_category(s)
                .map(ImpactCategoryKey)
                .ok_or_else(|| unknown_key(VocabKind::ImpactCategory, s))
        })
        .deserialize(d)
    }
}

/// A catalog scheme key, resolved as it is read.
struct CatalogSchemeKey(CatalogScheme);

impl<'de> Deserialize<'de> for CatalogSchemeKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .catalog_scheme(s)
                .map(CatalogSchemeKey)
                .ok_or_else(|| unknown_key(VocabKind::CatalogScheme, s))
        })
        .deserialize(d)
    }
}

/// A property set given as keys: at most 64 (the most properties a
/// methodology declares), none repeated, each resolved as it is read into
/// one 8-byte [`PropertySet`]. Within the list a cap or a repeat is
/// reported at once, the first unknown key when the list ends. Time
/// O(k log m), no allocation.
pub(super) struct PropertySetKeys(pub(super) PropertySet);

impl<'de> Deserialize<'de> for PropertySetKeys {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = PropertySetKeys;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of property keys")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<PropertySetKeys, A::Error> {
                let m = context().map_err(raise)?;
                let mut set = PropertySet::empty();
                let mut count = 0usize;
                let mut pending = None;
                while let Some(found) = seq.next_element_seed(WithStr::new(|s: &str| {
                    check_key_len(s)?;
                    Ok(note_unknown(
                        m.property(s),
                        VocabKind::Property,
                        s,
                        &mut pending,
                    ))
                }))? {
                    count += 1;
                    if count > MAX_PROPERTIES {
                        return Err(raise(Fault::Limit(Limit::PROPERTIES)));
                    }
                    if let Some(p) = found {
                        if set.contains(p) {
                            return Err(raise(Fault::Duplicate));
                        }
                        set = set.with(p);
                    }
                }
                match pending {
                    Some(fault) => Err(raise(fault)),
                    None => Ok(PropertySetKeys(set)),
                }
            }
        }
        d.deserialize_seq(V)
    }
}

/// Impact ratings given as an object from category key to level key, in
/// input order: at most 32 entries (the most categories a methodology
/// declares), no category repeated, each pair resolved as it is read.
/// Within the object a cap or a repeat is reported at once, the first
/// unknown key when the object ends. Time O(c log m); one allocation of 2
/// bytes per rating.
pub(super) struct Impacts(pub(super) Vec<(ImpactCategory, Level)>);

impl<'de> Deserialize<'de> for Impacts {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Impacts;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of impact ratings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Impacts, A::Error> {
                let m = context().map_err(raise)?;
                let mut out = Vec::new();
                let mut seen = 0u64;
                let mut count = 0usize;
                let mut pending = None;
                while let Some(category) = map.next_key_seed(WithStr::new(|s: &str| {
                    check_key_len(s)?;
                    Ok(note_unknown(
                        m.impact_category(s),
                        VocabKind::ImpactCategory,
                        s,
                        &mut pending,
                    ))
                }))? {
                    count += 1;
                    if count > MAX_IMPACT_CATEGORIES {
                        return Err(raise(Fault::Limit(Limit::IMPACTS)));
                    }
                    if let Some(c) = category {
                        let bit = u32::try_from(c.index())
                            .ok()
                            .and_then(|i| 1u64.checked_shl(i))
                            .unwrap_or(0);
                        if bit == 0 || seen & bit != 0 {
                            return Err(raise(Fault::Duplicate));
                        }
                        seen |= bit;
                    }
                    let level = map.next_value_seed(WithStr::new(|s: &str| {
                        check_key_len(s)?;
                        Ok(category.and_then(|c| {
                            note_unknown(m.level(c, s), VocabKind::Level, s, &mut pending)
                        }))
                    }))?;
                    if let (Some(c), Some(l)) = (category, level) {
                        out.push((c, l));
                    }
                }
                match pending {
                    Some(fault) => Err(raise(fault)),
                    None => Ok(Impacts(out)),
                }
            }
        }
        d.deserialize_map(V)
    }
}

/// `{"scheme": <key>, "id": <text>}` as read: the scheme already resolved.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalogRef {
    scheme: CatalogSchemeKey,
    id: RefText,
}

/// A catalog ref: the scheme resolved, then the text parsed again under the
/// scheme's syntax. The text (at most 64 bytes) is dropped right away.
/// Time O(log m + text length).
pub(super) struct CatalogRefText(pub(super) CatalogRef);

impl<'de> Deserialize<'de> for CatalogRefText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawCatalogRef::deserialize(d)?;
        let m = context().map_err(raise)?;
        m.parse_catalog_ref(raw.scheme.0, &raw.id.0)
            .map(CatalogRefText)
            .map_err(|_| {
                raise(Fault::InvalidCatalogRef {
                    scheme: Arc::from(m.catalog_scheme_key(raw.scheme.0).unwrap_or("")),
                })
            })
    }
}

/// `{"key": ..., "version": ...}` as read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMethodology {
    key: Key,
    version: VersionText,
}

/// The file's methodology, checked as it is read: the key must equal the
/// supplied methodology's key, else [`StoreError::MethodologyMismatch`]
/// before any vocabulary key is looked up. Any version is accepted (ADR
/// 0009, open question 1) and kept.
pub(super) struct CheckedMethodology {
    pub(super) version: Arc<str>,
}

impl<'de> Deserialize<'de> for CheckedMethodology {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawMethodology::deserialize(d)?;
        let m = context().map_err(raise)?;
        if &*raw.key.0 == m.key() {
            Ok(CheckedMethodology {
                version: raw.version.0,
            })
        } else {
            Err(raise(Fault::MethodologyMismatch {
                expected: Arc::from(m.key()),
                found: raw.key.0,
            }))
        }
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
        WithStr::new(|s: &str| {
            if !is_canonical_uuid(s) {
                return Err(Fault::InvalidId);
            }
            T::from_str(s).map(WireId).map_err(|_| Fault::InvalidId)
        })
        .deserialize(d)
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
        WithStr::new(|s: &str| {
            if s == F::NAME {
                Ok(Format(PhantomData))
            } else {
                Err(Fault::UnsupportedFormat(None))
            }
        })
        .deserialize(d)
    }
}

/// The format `version` field: exactly [`FORMAT_VERSION`]; any other
/// number is [`StoreError::UnsupportedFormat`] as soon as it is read,
/// carrying the number only if it is an integer above the current version
/// (a newer file), else `None` (0, negative, or fractional: not a valid
/// file).
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
                match v.cmp(&u64::from(FORMAT_VERSION)) {
                    std::cmp::Ordering::Equal => Ok(FormatVersion),
                    // Most likely written by a newer application.
                    std::cmp::Ordering::Greater => Err(raise(Fault::UnsupportedFormat(Some(v)))),
                    // Below every version ever written: not a valid file.
                    std::cmp::Ordering::Less => Err(raise(Fault::UnsupportedFormat(None))),
                }
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<FormatVersion, E> {
                Err(raise(Fault::UnsupportedFormat(None)))
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<FormatVersion, E> {
                Err(raise(Fault::UnsupportedFormat(None)))
            }
        }
        d.deserialize_u64(V)
    }
}

// ---------------------------------------------------------------------------
// Owned wire types (reading).
// ---------------------------------------------------------------------------

/// A snapshot. The methodology field is checked while decoding.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SnapshotWire {
    // Checked while decoding, then not needed.
    #[serde(rename = "format")]
    _format: Format<ProjectFormat>,
    #[serde(rename = "version")]
    _version: FormatVersion,
    #[serde(rename = "methodology")]
    _methodology: CheckedMethodology,
    pub(super) assets: Vec<AssetWire>,
    pub(super) damage_scenarios: Vec<DamageWire>,
    pub(super) threat_scenarios: Vec<ThreatWire>,
}

/// A snapshot asset: 40 bytes plus its name.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssetWire {
    pub(super) id: WireId<AssetId>,
    pub(super) name: Name,
    pub(super) properties: PropertySetKeys,
}

/// A snapshot damage scenario; unrated categories are absent.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageWire {
    pub(super) id: WireId<DamageId>,
    pub(super) name: Name,
    pub(super) impacts: Impacts,
}

/// A snapshot threat scenario.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThreatWire {
    pub(super) id: WireId<ThreatId>,
    pub(super) name: Name,
    pub(super) asset: WireId<AssetId>,
    pub(super) category: ThreatCategoryKey,
    pub(super) catalog_refs: BoundedVec<CatalogRefText, RefsCap>,
    pub(super) damages: BoundedVec<WireId<DamageId>, LinksCap>,
}

/// The op log header line. The methodology is checked while decoding.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeaderWire {
    // Checked while decoding, then not needed.
    #[serde(rename = "format")]
    _format: Format<OpLogFormat>,
    #[serde(rename = "version")]
    _version: FormatVersion,
    pub(super) methodology: CheckedMethodology,
}

/// One op log record line.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordWire {
    pub(super) seq: u64,
    pub(super) op: OpWire,
}

/// An op, internally tagged by `"op"`. Field names are the `Op` field
/// names. Vocabulary is resolved while decoding, except the `SetImpact`
/// level, which is resolved against its category afterwards.
#[derive(serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OpWire {
    AddAsset {
        id: WireId<AssetId>,
        name: Name,
        properties: PropertySetKeys,
    },
    RenameAsset {
        id: WireId<AssetId>,
        name: Name,
    },
    SetAssetProperties {
        id: WireId<AssetId>,
        properties: PropertySetKeys,
    },
    RemoveAsset {
        id: WireId<AssetId>,
    },
    AddDamageScenario {
        id: WireId<DamageId>,
        name: Name,
        impacts: Impacts,
    },
    RenameDamageScenario {
        id: WireId<DamageId>,
        name: Name,
    },
    SetImpact {
        id: WireId<DamageId>,
        category: ImpactCategoryKey,
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
        category: ThreatCategoryKey,
        catalog_refs: BoundedVec<CatalogRefText, RefsCap>,
        damages: BoundedVec<WireId<DamageId>, LinksCap>,
    },
    RenameThreatScenario {
        id: WireId<ThreatId>,
        name: Name,
    },
    SetThreatCategory {
        id: WireId<ThreatId>,
        category: ThreatCategoryKey,
    },
    AddCatalogRef {
        threat: WireId<ThreatId>,
        catalog_ref: CatalogRefText,
    },
    RemoveCatalogRef {
        threat: WireId<ThreatId>,
        catalog_ref: CatalogRefText,
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

/// Resolves the `SetImpact` level key within its category. Time O(log m).
pub(super) fn resolve_level(
    m: &Methodology,
    cat: ImpactCategory,
    key: &Key,
) -> Result<Level, StoreError> {
    m.level(cat, &key.0).ok_or_else(|| StoreError::UnknownKey {
        kind: VocabKind::Level,
        key: Arc::clone(&key.0),
    })
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

    /// Decodes `json` against the second test methodology.
    fn de<T: for<'a> Deserialize<'a>>(json: &str) -> Result<T, StoreError> {
        from_slice(&testkit::second(), json.as_bytes(), Some(7))
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
    fn context_is_scoped_to_one_decode() {
        assert_eq!(context().err(), Some(Fault::NoContext));
        let m = testkit::second();
        {
            let _outer = ContextGuard::enter(&m);
            let other = testkit::builtin();
            {
                let _inner = ContextGuard::enter(&other);
                assert_eq!(
                    context().map(|c| c.key().to_string()).ok(),
                    Some(other.key().into())
                );
            }
            assert_eq!(
                context().map(|c| c.key().to_string()).ok(),
                Some(m.key().into())
            );
        }
        assert_eq!(context().err(), Some(Fault::NoContext));
        assert!(matches!(
            StoreError::from(Fault::NoContext),
            StoreError::Decode { line: 0, column: 0 }
        ));
        // Outside `decode`, a resolving leaf fails instead of guessing.
        let mut d = serde_json::Deserializer::from_str("\"outage\"");
        assert!(ThreatCategoryKey::deserialize(&mut d).is_err());
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
    fn property_keys_resolve_while_decoding() {
        let m = testkit::second();
        let set = de::<PropertySetKeys>(r#"["i","a"]"#).map(|s| s.0).ok();
        assert_eq!(set, Some(testkit::props(&m, &["a", "i"])));
        assert!(matches!(
            de::<PropertySetKeys>(r#"["a","i","a"]"#).err(),
            Some(StoreError::DuplicateEntry)
        ));
        // The first unknown key is reported, at the end of the list.
        assert!(matches!(
            de::<PropertySetKeys>(r#"["a","x","y"]"#).err(),
            Some(StoreError::UnknownKey { kind: VocabKind::Property, key }) if &*key == "x"
        ));
        // A cap in the same list wins over an unknown key.
        let over = (0..65)
            .map(|i| format!("\"k{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        assert!(matches!(
            de::<PropertySetKeys>(&format!("[{over}]")).err(),
            Some(StoreError::LimitExceeded(Limit::Properties { max: 64 }))
        ));
    }

    #[test]
    fn impacts_resolve_while_decoding_and_keep_order() {
        let m = testkit::second();
        let (harm, loss) = (
            testkit::cat(&m, "patient-harm"),
            testkit::cat(&m, "service-loss"),
        );
        let got = de::<Impacts>(r#"{"service-loss":"tolerable","patient-harm":"minor"}"#)
            .map(|i| i.0)
            .ok();
        assert_eq!(
            got,
            Some(vec![
                (loss, testkit::lvl(&m, loss, "tolerable")),
                (harm, testkit::lvl(&m, harm, "minor")),
            ])
        );
        assert!(matches!(
            de::<Impacts>(r#"{"patient-harm":"minor","patient-harm":"serious"}"#).err(),
            Some(StoreError::DuplicateEntry)
        ));
        assert!(matches!(
            de::<Impacts>(r#"{"patient-harm":"huge"}"#).err(),
            Some(StoreError::UnknownKey { kind: VocabKind::Level, key }) if &*key == "huge"
        ));
        assert!(matches!(
            de::<Impacts>(r#"{"nope":"minor"}"#).err(),
            Some(StoreError::UnknownKey {
                kind: VocabKind::ImpactCategory,
                ..
            })
        ));
        let over = (0..33)
            .map(|i| format!("\"c{i}\":\"x\""))
            .collect::<Vec<_>>()
            .join(",");
        assert!(matches!(
            de::<Impacts>(&format!("{{{over}}}")).err(),
            Some(StoreError::LimitExceeded(Limit::Impacts { max: 32 }))
        ));
    }

    #[test]
    fn scalar_keys_and_catalog_refs_resolve_while_decoding() {
        let m = testkit::second();
        assert_eq!(
            de::<ThreatCategoryKey>("\"outage\"").map(|k| k.0).ok(),
            Some(testkit::threat_cat(&m, "outage"))
        );
        assert!(matches!(
            de::<ThreatCategoryKey>("\"nope\"").err(),
            Some(StoreError::UnknownKey {
                kind: VocabKind::ThreatCategory,
                ..
            })
        ));
        assert!(matches!(
            de::<ImpactCategoryKey>("\"nope\"").err(),
            Some(StoreError::UnknownKey {
                kind: VocabKind::ImpactCategory,
                ..
            })
        ));
        let capec = testkit::scheme(&m, "capec");
        assert_eq!(
            de::<CatalogRefText>(r#"{"scheme":"capec","id":"CAPEC-7"}"#)
                .map(|r| r.0)
                .ok(),
            Some(testkit::cref(&m, capec, "CAPEC-7"))
        );
        assert!(matches!(
            de::<CatalogRefText>(r#"{"scheme":"capec","id":"7"}"#).err(),
            Some(StoreError::InvalidCatalogRef { scheme }) if &*scheme == "capec"
        ));
        assert!(matches!(
            de::<CatalogRefText>(r#"{"scheme":"nope","id":"7"}"#).err(),
            Some(StoreError::UnknownKey {
                kind: VocabKind::CatalogScheme,
                ..
            })
        ));
    }

    #[test]
    fn methodology_is_checked_while_decoding() {
        let ok = de::<CheckedMethodology>(r#"{"key":"med-device-demo","version":"9"}"#);
        assert_eq!(ok.map(|c| c.version.to_string()).ok(), Some("9".into()));
        assert!(matches!(
            de::<CheckedMethodology>(r#"{"key":"other","version":"1"}"#).err(),
            Some(StoreError::MethodologyMismatch { found, .. }) if &*found == "other"
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
                Some(StoreError::UnsupportedFormat { version: None })
            ));
        }
        assert!(de::<FormatVersion>("1").is_ok());
        assert!(matches!(
            de::<FormatVersion>("2").err(),
            Some(StoreError::UnsupportedFormat { version: Some(2) })
        ));
        for bad in ["-1", "1.5"] {
            assert!(matches!(
                de::<FormatVersion>(bad).err(),
                Some(StoreError::UnsupportedFormat { version: None })
            ));
        }
        // Version 0 is not "newer": no message may suggest updating.
        assert!(matches!(
            de::<FormatVersion>("0").err(),
            Some(StoreError::UnsupportedFormat { version: None })
        ));
        let text = de::<FormatVersion>("0").err().map(|e| e.to_string());
        assert!(text.is_some_and(|t| !t.contains("newer")));
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
}
