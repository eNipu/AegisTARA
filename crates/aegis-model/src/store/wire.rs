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
//! Two thread-local cells connect the visitors with the two entry points,
//! [`decode`] (snapshots and op logs) and [`decode_pack`] (methodology
//! packs, which need no methodology context):
//!
//! - **Context.** `decode` installs the methodology for the duration of one
//!   decode (an RAII guard restores the previous value, also on unwind).
//!   This is the `DeserializeSeed` idea without threading a seed through
//!   every derived struct: derive keeps its strict field handling
//!   (`deny_unknown_fields`, duplicate and missing fields) for all the wire
//!   shapes, including the internally tagged ops. The pack wire types
//!   (`crate::methodology::pack`) use only the bounded leaves and the
//!   format checks, none of which read the context.
//! - **Faults.** A cap, a repeated key, a bad ID, an unknown key, a bad
//!   catalog ref, a wrong format, or another methodology found inside a
//!   visitor must come out as its own [`StoreError`] (or [`PackError`])
//!   variant, but serde only carries a text message. The visitor records a
//!   [`Fault`] before it returns a fixed-text serde error, and the entry
//!   point reads it back. The cell is cleared before each decode and taken
//!   once after it, on the same thread, so a fault can never leak from one
//!   decode into another.
//!
//! The bounded leaves ([`BoundedStr`], [`BoundedVec`], [`Cap`]), the format
//! checks ([`Format`], [`FormatVersion`]), [`CappedReader`], and
//! [`decode_pack`] are `pub(crate)` so the pack reader shares one strict,
//! capped decoding approach with the store (ADR 0011).

use std::cell::{Cell, RefCell};
use std::fmt;
use std::io::Read;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::Arc;

use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, Serialize, SerializeMap, SerializeStruct, Serializer};

use super::{Limit, MAX_KEY_BYTES, VocabKind};
use crate::asset::PropertySet;
use crate::control::{
    MAX_CONTROL_RULES, MAX_CONTROL_TARGETS, MAX_GOAL_CONTROLS, MAX_STEP_MITIGATIONS, RiskTreatment,
    StepMitigation,
};
use crate::error::{PackError, StoreError};
use crate::id::{
    AssetId, ClaimId, ComponentId, ConditionId, ControlId, DamageId, DataFlowId, ElementId, GoalId,
    InterfaceId, NodeId, NodeIdx, PartyId, ThreatId, TreeId, TreeIdx, TrustBoundaryId,
    VisualPrefix,
};
use crate::item::ItemRef;
use crate::methodology::{
    AttackType, CatalogRef, CatalogScheme, ControlKind, FactorLevel, FeasibilityFactor,
    FeasibilityLevel, ImpactCategory, InterfaceKind, Level, MAX_FEASIBILITY_FACTORS,
    MAX_IMPACT_CATEGORIES, MAX_PROPERTIES, Methodology, Property, ThreatCategory,
};
use crate::provenance::{Provenance, ProvenanceSource};
use crate::tree::{AttackTree, GateKind, MAX_ATTACK_TREE_NODES, Node, WireNodeParts};

/// The version every format (snapshot, op log, methodology pack) is written
/// with and the only one read.
pub(crate) const FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Decode context and fault side channel.
// ---------------------------------------------------------------------------

/// A typed decode failure found inside a serde visitor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
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
    let result = with_fault_channel(f);
    drop(guard);
    result.map_err(|(e, fault)| match fault {
        Some(fault) => fault.into(),
        None if e.is_io() => StoreError::Io(e.into()),
        None => StoreError::Decode {
            line: line.unwrap_or_else(|| to_u64(e.line())),
            column: to_u64(e.column()),
        },
    })
}

/// Runs `f` with a clean fault cell and returns its error together with
/// the fault a visitor recorded, if any. Time O(1) besides `f`.
fn with_fault_channel<T>(
    f: impl FnOnce() -> Result<T, serde_json::Error>,
) -> Result<T, (serde_json::Error, Option<Fault>)> {
    FAULT.with(|c| c.set(None));
    let result = f();
    let fault = FAULT.with(Cell::take);
    result.map_err(|e| (e, fault))
}

/// Runs one serde decode of a methodology pack, which needs no methodology
/// context, and maps its error: a recorded cap or format [`Fault`] wins; an
/// I/O error stays [`PackError::Io`]; any other error becomes
/// [`PackError::Decode`] with its position only (the parser's message can
/// quote input). Time O(1) besides `f`.
pub(crate) fn decode_pack<T>(
    f: impl FnOnce() -> Result<T, serde_json::Error>,
) -> Result<T, PackError> {
    with_fault_channel(f).map_err(|(e, fault)| match fault {
        Some(Fault::Limit(l)) => PackError::LimitExceeded(l),
        Some(Fault::UnsupportedFormat(version)) => PackError::UnsupportedFormat { version },
        _ if e.is_io() => PackError::Io(e.into()),
        // The pack wire types raise no other fault; keep the position.
        _ => PackError::Decode {
            line: to_u64(e.line()),
            column: to_u64(e.column()),
        },
    })
}

/// Widens a `usize` (always fits on supported targets; saturates otherwise).
pub(crate) fn to_u64(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// A reader that fails once more than `cap` bytes have come through, and
/// remembers that it did, so the caller can report its file-size
/// [`Limit`] instead of the parser's I/O error. It never pulls more than
/// `cap + 1` bytes from the inner reader. Time O(1) per call besides the
/// inner read.
pub(crate) struct CappedReader<R> {
    inner: std::io::Take<R>,
    cap: u64,
    read: u64,
    exceeded: bool,
}

impl<R: Read> CappedReader<R> {
    pub(crate) fn new(inner: R, cap: u64) -> Self {
        CappedReader {
            inner: inner.take(cap.saturating_add(1)),
            cap,
            read: 0,
            exceeded: false,
        }
    }

    /// Whether more than the cap came through.
    pub(crate) fn exceeded(&self) -> bool {
        self.exceeded
    }

    /// Bytes pulled from the inner reader so far.
    #[cfg(test)]
    pub(crate) fn bytes_read(&self) -> u64 {
        self.read
    }
}

impl<R: Read> Read for CappedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read = self.read.saturating_add(to_u64(n));
        if self.read > self.cap {
            self.exceeded = true;
            return Err(std::io::Error::other("input exceeds its size cap"));
        }
        Ok(n)
    }
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
pub(crate) trait Cap {
    const MAX: usize;
    const LIMIT: Limit;
}

macro_rules! caps {
    ($($name:ident = $max:expr, $limit:expr;)*) => {$(
        pub(crate) enum $name {}
        impl Cap for $name {
            const MAX: usize = $max;
            const LIMIT: Limit = $limit;
        }
    )*};
}

caps! {
    NameCap = super::MAX_NAME_BYTES, Limit::NAME;
    DescriptionCap = super::MAX_DESCRIPTION_BYTES, Limit::DESCRIPTION;
    LangCap = crate::description::MAX_LANG_TAG_BYTES, Limit::LANG;
    ActorCap = super::MAX_ACTOR_BYTES, Limit::ACTOR;
    KeyCap = super::MAX_KEY_BYTES, Limit::KEY;
    VersionCap = super::MAX_METHODOLOGY_VERSION_BYTES, Limit::VERSION;
    RefTextCap = super::MAX_CATALOG_REF_BYTES, Limit::CATALOG_REF;
    RefsCap = crate::threat::MAX_CATALOG_REFS, Limit::CATALOG_REFS;
    LinksCap = crate::threat::MAX_DAMAGE_LINKS, Limit::DAMAGE_LINKS;
    AttackTreeNodesCap = crate::tree::MAX_ATTACK_TREE_NODES, Limit::ATTACK_TREE_NODES;
    GoalControlsCap = MAX_GOAL_CONTROLS, Limit::GOAL_CONTROLS;
    ResponsibleCap = crate::party::MAX_RESPONSIBLE, Limit::RESPONSIBLE;
    PartiesCap = crate::party::MAX_PARTIES, Limit::PARTIES;
    ConditionsCap = crate::applicability::MAX_CONDITIONS, Limit::CONDITIONS;
    ConditionLinksCap = crate::applicability::MAX_CONDITIONS_PER_ELEMENT, Limit::CONDITION_LINKS;
    BoundaryMembersCap = crate::item::MAX_TRUST_BOUNDARY_MEMBERS, Limit::TRUST_BOUNDARY_MEMBERS;
    ElementsCap = crate::element::MAX_ELEMENTS_PER_KIND, Limit::ELEMENTS;
    ElementAttributesCap = crate::element::MAX_ELEMENT_ATTRIBUTES, Limit::ELEMENT_ATTRIBUTES;
    ElementLinksCap = crate::element::MAX_ELEMENT_LINKS, Limit::ELEMENT_LINKS;
}

/// A string of at most `C::MAX` bytes, kept. The check runs in the visitor,
/// on the parser's text, before anything is allocated.
pub(crate) struct BoundedStr<C>(pub(crate) Arc<str>, PhantomData<C>);

pub(super) type Name = BoundedStr<NameCap>;
pub(super) type DescriptionText = BoundedStr<DescriptionCap>;
pub(super) type LangText = BoundedStr<LangCap>;
pub(super) type Actor = BoundedStr<ActorCap>;
pub(crate) type Key = BoundedStr<KeyCap>;
pub(crate) type VersionText = BoundedStr<VersionCap>;
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
pub(crate) struct BoundedVec<T, C>(pub(crate) Vec<T>, PhantomData<C>);

impl<T, C> Default for BoundedVec<T, C> {
    fn default() -> Self {
        BoundedVec(Vec::new(), PhantomData)
    }
}

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

/// A map from [`Key`] to `T` of at most `C::MAX` entries, rejecting repeated
/// keys.
pub(crate) struct BoundedKeyMap<T, C>(
    pub(crate) std::collections::BTreeMap<Arc<str>, T>,
    PhantomData<C>,
);

impl<T, C> Default for BoundedKeyMap<T, C> {
    fn default() -> Self {
        BoundedKeyMap(std::collections::BTreeMap::new(), PhantomData)
    }
}

impl<'de, T: Deserialize<'de>, C: Cap> Deserialize<'de> for BoundedKeyMap<T, C> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T, C>(PhantomData<(T, C)>);
        impl<'de, T: Deserialize<'de>, C: Cap> Visitor<'de> for V<T, C> {
            type Value = BoundedKeyMap<T, C>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut items = std::collections::BTreeMap::new();
                while let Some(key) = map.next_key::<Key>()? {
                    if items.len() >= C::MAX {
                        return Err(raise(Fault::Limit(C::LIMIT)));
                    }
                    let val = map.next_value::<T>()?;
                    if items.insert(key.0, val).is_some() {
                        return Err(de::Error::custom("duplicate key"));
                    }
                }
                Ok(BoundedKeyMap(items, PhantomData))
            }
        }
        d.deserialize_map(V(PhantomData))
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

/// An element kind key, resolved as it is read.
pub(super) struct ElementKindKey(pub(super) crate::element::ElementKind);

impl<'de> Deserialize<'de> for ElementKindKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            crate::element::ElementKind::from_key(s)
                .map(ElementKindKey)
                .ok_or_else(|| unknown_key(VocabKind::ElementKind, s))
        })
        .deserialize(d)
    }
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

/// A feasibility level key, resolved as it is read.
pub(super) struct FeasibilityLevelKey(pub(super) FeasibilityLevel);

impl<'de> Deserialize<'de> for FeasibilityLevelKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .feasibility_level(s)
                .map(FeasibilityLevelKey)
                .ok_or_else(|| unknown_key(VocabKind::FeasibilityLevel, s))
        })
        .deserialize(d)
    }
}

/// A feasibility factor key, resolved as it is read.
pub(super) struct FeasibilityFactorKey(pub(super) FeasibilityFactor);

impl<'de> Deserialize<'de> for FeasibilityFactorKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .feasibility_factor(s)
                .map(FeasibilityFactorKey)
                .ok_or_else(|| unknown_key(VocabKind::FeasibilityFactor, s))
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

/// An interface kind key, resolved as it is read.
pub(super) struct InterfaceKindKey(pub(super) InterfaceKind);

impl<'de> Deserialize<'de> for InterfaceKindKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .interface_kind(s)
                .map(InterfaceKindKey)
                .ok_or_else(|| unknown_key(VocabKind::InterfaceKind, s))
        })
        .deserialize(d)
    }
}

/// A control kind key, resolved as it is read.
pub(super) struct ControlKindKey(pub(super) ControlKind);

impl<'de> Deserialize<'de> for ControlKindKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .control_kind(s)
                .map(ControlKindKey)
                .ok_or_else(|| unknown_key(VocabKind::ControlKind, s))
        })
        .deserialize(d)
    }
}

/// An attack type key, resolved as it is read.
pub(super) struct AttackTypeKey(pub(super) AttackType);

impl<'de> Deserialize<'de> for AttackTypeKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            check_key_len(s)?;
            context()?
                .attack_type(s)
                .map(AttackTypeKey)
                .ok_or_else(|| unknown_key(VocabKind::AttackType, s))
        })
        .deserialize(d)
    }
}

/// A "through" item on the wire: `{"component": id}` or `{"interface": id}`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ItemWire {
    Component(WireId<ComponentId>),
    Interface(WireId<InterfaceId>),
}

impl ItemWire {
    pub(super) fn unwrap_item(self) -> ItemRef {
        match self {
            ItemWire::Component(id) => ItemRef::Component(id.0),
            ItemWire::Interface(id) => ItemRef::Interface(id.0),
        }
    }
}

/// The writing side of [`ItemWire`].
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ItemView {
    Component(Text<ComponentId>),
    Interface(Text<InterfaceId>),
}

impl ItemView {
    pub(super) fn of(item: ItemRef) -> Self {
        match item {
            ItemRef::Component(id) => ItemView::Component(Text(id)),
            ItemRef::Interface(id) => ItemView::Interface(Text(id)),
        }
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

/// Step feasibility ratings given as an object from step `NodeId` to
/// feasibility level key, in input order: at most 32 entries
/// ([`MAX_ATTACK_TREE_NODES`]), no node ID repeated, each pair resolved as
/// it is read. Within the object a cap, an invalid ID, or a repeat is
/// reported at once, the first unknown key when the object ends. Time
/// O(k log f).
pub(super) struct StepFeasibility(pub(super) Vec<(NodeId, FeasibilityLevel)>);

impl<'de> Deserialize<'de> for StepFeasibility {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StepFeasibility;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of step feasibility ratings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StepFeasibility, A::Error> {
                let m = context().map_err(raise)?;
                let mut out = Vec::new();
                let mut seen: Vec<NodeId> = Vec::new();
                let mut count = 0usize;
                let mut pending = None;
                while let Some(node) = map.next_key_seed(WithStr::new(|s: &str| {
                    if count >= MAX_ATTACK_TREE_NODES {
                        return Err(Fault::Limit(Limit::STEP_FEASIBILITY));
                    }
                    if !is_canonical_uuid(s) {
                        return Err(Fault::InvalidId);
                    }
                    NodeId::from_str(s).map_err(|_| Fault::InvalidId)
                }))? {
                    count += 1;
                    if seen.contains(&node) {
                        return Err(raise(Fault::Duplicate));
                    }
                    seen.push(node);
                    let level = map.next_value_seed(WithStr::new(|s: &str| {
                        check_key_len(s)?;
                        Ok(note_unknown(
                            m.feasibility_level(s),
                            VocabKind::FeasibilityLevel,
                            s,
                            &mut pending,
                        ))
                    }))?;
                    if let Some(l) = level {
                        out.push((node, l));
                    }
                }
                match pending {
                    Some(fault) => Err(raise(fault)),
                    None => Ok(StepFeasibility(out)),
                }
            }
        }
        d.deserialize_map(V)
    }
}

/// Step mitigations for one node given as an object from `ControlId` to
/// feasibility level key: at most 16 entries ([`MAX_STEP_MITIGATIONS`]),
/// no control ID repeated, each pair resolved as it is read.
pub(super) struct NodeMitigations(pub(super) Vec<(ControlId, FeasibilityLevel)>);

impl<'de> Deserialize<'de> for NodeMitigations {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = NodeMitigations;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of step mitigations")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NodeMitigations, A::Error> {
                let m = context().map_err(raise)?;
                let mut out = Vec::new();
                let mut seen: Vec<ControlId> = Vec::new();
                let mut count = 0usize;
                let mut pending = None;
                while let Some(control) = map.next_key_seed(WithStr::new(|s: &str| {
                    if count >= MAX_STEP_MITIGATIONS {
                        return Err(Fault::Limit(Limit::STEP_MITIGATIONS));
                    }
                    if !is_canonical_uuid(s) {
                        return Err(Fault::InvalidId);
                    }
                    ControlId::from_str(s).map_err(|_| Fault::InvalidId)
                }))? {
                    count += 1;
                    if seen.contains(&control) {
                        return Err(raise(Fault::Duplicate));
                    }
                    seen.push(control);
                    let level = map.next_value_seed(WithStr::new(|s: &str| {
                        check_key_len(s)?;
                        Ok(note_unknown(
                            m.feasibility_level(s),
                            VocabKind::FeasibilityLevel,
                            s,
                            &mut pending,
                        ))
                    }))?;
                    if let Some(l) = level {
                        out.push((control, l));
                    }
                }
                match pending {
                    Some(fault) => Err(raise(fault)),
                    None if count == 0 => Err(de::Error::custom(
                        "step_mitigations entry for a step must not be empty",
                    )),
                    None => Ok(NodeMitigations(out)),
                }
            }
        }
        d.deserialize_map(V)
    }
}

/// Step mitigations across an attack tree given as an object from step
/// `NodeId` to [`NodeMitigations`]: at most 32 entries
/// ([`MAX_ATTACK_TREE_NODES`]), no node ID repeated.
#[derive(Default)]
pub(super) struct StepMitigations(pub(super) Vec<(NodeId, Vec<(ControlId, FeasibilityLevel)>)>);

impl<'de> Deserialize<'de> for StepMitigations {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StepMitigations;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of step mitigations by node")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StepMitigations, A::Error> {
                let mut out = Vec::new();
                let mut seen: Vec<NodeId> = Vec::new();
                let mut count = 0usize;
                while let Some(node) = map.next_key_seed(WithStr::new(|s: &str| {
                    if count >= MAX_ATTACK_TREE_NODES {
                        return Err(Fault::Limit(Limit::ATTACK_TREE_NODES));
                    }
                    if !is_canonical_uuid(s) {
                        return Err(Fault::InvalidId);
                    }
                    NodeId::from_str(s).map_err(|_| Fault::InvalidId)
                }))? {
                    count += 1;
                    if seen.contains(&node) {
                        return Err(raise(Fault::Duplicate));
                    }
                    seen.push(node);
                    let inner: NodeMitigations = map.next_value()?;
                    out.push((node, inner.0));
                }
                if count == 0 {
                    return Err(de::Error::custom(
                        "step_mitigations must be omitted when empty",
                    ));
                }
                Ok(StepMitigations(out))
            }
        }
        d.deserialize_map(V)
    }
}

/// Per-step values across an attack tree given as an object from step
/// `NodeId` to a value: at most 32 entries ([`MAX_ATTACK_TREE_NODES`]), no
/// node ID repeated, never empty (omit the field instead). `what` names the
/// field in the empty-object error.
pub(super) struct StepValues<V>(pub(super) Vec<(NodeId, V)>);

impl<'de, V: Deserialize<'de>> Deserialize<'de> for StepValues<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Vis<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for Vis<V> {
            type Value = StepValues<V>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of per-step values by node")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StepValues<V>, A::Error> {
                let mut out = Vec::new();
                let mut seen: Vec<NodeId> = Vec::new();
                while let Some(node) = map.next_key_seed(WithStr::new(|s: &str| {
                    if out.len() >= MAX_ATTACK_TREE_NODES {
                        return Err(Fault::Limit(Limit::ATTACK_TREE_NODES));
                    }
                    if !is_canonical_uuid(s) {
                        return Err(Fault::InvalidId);
                    }
                    NodeId::from_str(s).map_err(|_| Fault::InvalidId)
                }))? {
                    if seen.contains(&node) {
                        return Err(raise(Fault::Duplicate));
                    }
                    seen.push(node);
                    out.push((node, map.next_value()?));
                }
                if out.is_empty() {
                    return Err(de::Error::custom(
                        "per-step values must be omitted when empty",
                    ));
                }
                Ok(StepValues(out))
            }
        }
        d.deserialize_map(Vis(PhantomData))
    }
}

/// Step factor ratings for one node given as an object from feasibility
/// factor key to factor level key: at most 16 entries
/// ([`MAX_FEASIBILITY_FACTORS`]), no factor repeated, each pair resolved
/// as it is read.
pub(super) struct NodeFactors(pub(super) Vec<(FeasibilityFactor, FactorLevel)>);

impl<'de> Deserialize<'de> for NodeFactors {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = NodeFactors;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of step factor ratings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<NodeFactors, A::Error> {
                let m = context().map_err(raise)?;
                let mut out = Vec::new();
                let mut seen = 0u64;
                let mut count = 0usize;
                let mut pending = None;
                while let Some(factor) = map.next_key_seed(WithStr::new(|s: &str| {
                    check_key_len(s)?;
                    Ok(note_unknown(
                        m.feasibility_factor(s),
                        VocabKind::FeasibilityFactor,
                        s,
                        &mut pending,
                    ))
                }))? {
                    count += 1;
                    if count > MAX_FEASIBILITY_FACTORS {
                        return Err(raise(Fault::Limit(Limit::FEASIBILITY_FACTORS)));
                    }
                    if let Some(f) = factor {
                        let bit = u32::try_from(f.index())
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
                        Ok(factor.and_then(|f| {
                            note_unknown(
                                m.factor_level(f, s),
                                VocabKind::FactorLevel,
                                s,
                                &mut pending,
                            )
                        }))
                    }))?;
                    if let (Some(f), Some(l)) = (factor, level) {
                        out.push((f, l));
                    }
                }
                match pending {
                    Some(fault) => Err(raise(fault)),
                    None if count == 0 => Err(de::Error::custom(
                        "step_factors entry for a step must not be empty",
                    )),
                    None => Ok(NodeFactors(out)),
                }
            }
        }
        d.deserialize_map(V)
    }
}

/// Step factor ratings across an attack tree given as an object from step
/// `NodeId` to [`NodeFactors`]: at most 32 entries
/// ([`MAX_ATTACK_TREE_NODES`]), no node ID repeated.
#[derive(Default)]
pub(super) struct StepFactors(pub(super) Vec<(NodeId, Vec<(FeasibilityFactor, FactorLevel)>)>);

impl<'de> Deserialize<'de> for StepFactors {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StepFactors;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of step factors by node")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StepFactors, A::Error> {
                let mut out = Vec::new();
                let mut seen: Vec<NodeId> = Vec::new();
                let mut count = 0usize;
                while let Some(node) = map.next_key_seed(WithStr::new(|s: &str| {
                    if count >= MAX_ATTACK_TREE_NODES {
                        return Err(Fault::Limit(Limit::ATTACK_TREE_NODES));
                    }
                    if !is_canonical_uuid(s) {
                        return Err(Fault::InvalidId);
                    }
                    NodeId::from_str(s).map_err(|_| Fault::InvalidId)
                }))? {
                    count += 1;
                    if seen.contains(&node) {
                        return Err(raise(Fault::Duplicate));
                    }
                    seen.push(node);
                    let inner: NodeFactors = map.next_value()?;
                    out.push((node, inner.0));
                }
                if count == 0 {
                    return Err(de::Error::custom("step_factors must be omitted when empty"));
                }
                Ok(StepFactors(out))
            }
        }
        d.deserialize_map(V)
    }
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RiskTreatmentWire {
    Avoid,
    Reduce,
    Share,
    Retain,
}

impl From<RiskTreatment> for RiskTreatmentWire {
    fn from(t: RiskTreatment) -> Self {
        match t {
            RiskTreatment::Avoid => Self::Avoid,
            RiskTreatment::Reduce => Self::Reduce,
            RiskTreatment::Share => Self::Share,
            RiskTreatment::Retain => Self::Retain,
        }
    }
}

impl From<RiskTreatmentWire> for RiskTreatment {
    fn from(t: RiskTreatmentWire) -> Self {
        match t {
            RiskTreatmentWire::Avoid => Self::Avoid,
            RiskTreatmentWire::Reduce => Self::Reduce,
            RiskTreatmentWire::Share => Self::Share,
            RiskTreatmentWire::Retain => Self::Retain,
        }
    }
}

/// Deserializes `T` into `Some(T)`. Used with
/// `#[serde(default, deserialize_with = "some")]` so an omitted field
/// becomes `None` while an explicit `null` is rejected.
fn some<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProvenanceSourceWire {
    Human,
    Import,
    Catalog,
    Ai,
}

impl From<ProvenanceSource> for ProvenanceSourceWire {
    fn from(s: ProvenanceSource) -> Self {
        match s {
            ProvenanceSource::Human => Self::Human,
            ProvenanceSource::Import => Self::Import,
            ProvenanceSource::Catalog => Self::Catalog,
            ProvenanceSource::Ai => Self::Ai,
        }
    }
}

impl From<ProvenanceSourceWire> for ProvenanceSource {
    fn from(s: ProvenanceSourceWire) -> Self {
        match s {
            ProvenanceSourceWire::Human => Self::Human,
            ProvenanceSourceWire::Import => Self::Import,
            ProvenanceSourceWire::Catalog => Self::Catalog,
            ProvenanceSourceWire::Ai => Self::Ai,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProvenance {
    actor: Actor,
    timestamp_ms: u64,
    source: ProvenanceSourceWire,
}

/// Non-default provenance decoded from
/// `{"actor": ..., "timestamp_ms": ..., "source": ...}`. Explicit default
/// provenance (`{"actor":"","timestamp_ms":0,"source":"human"}`) is
/// rejected so default provenance has one canonical form (omitted).
pub(super) struct ProvenanceWire(pub(super) Provenance);

impl<'de> Deserialize<'de> for ProvenanceWire {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = RawProvenance::deserialize(d)?;
        let prov = Provenance::new(raw.actor.0, raw.timestamp_ms, raw.source.into());
        if prov.is_default() {
            return Err(de::Error::custom("default provenance must be omitted"));
        }
        Ok(ProvenanceWire(prov))
    }
}

pub(super) fn unwrap_provenance(p: Option<ProvenanceWire>) -> Provenance {
    p.map_or_else(Provenance::default, |w| w.0)
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GateKindWire {
    And,
    Or,
    SeqAnd,
}

impl From<GateKind> for GateKindWire {
    fn from(k: GateKind) -> Self {
        match k {
            GateKind::And => Self::And,
            GateKind::Or => Self::Or,
            GateKind::SeqAnd => Self::SeqAnd,
        }
    }
}

impl From<GateKindWire> for GateKind {
    fn from(k: GateKindWire) -> Self {
        match k {
            GateKindWire::And => Self::And,
            GateKindWire::Or => Self::Or,
            GateKindWire::SeqAnd => Self::SeqAnd,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeWire {
    id: WireId<NodeId>,
    label: Name,
    #[serde(default, deserialize_with = "some")]
    gate: Option<GateKindWire>,
    #[serde(default, deserialize_with = "some")]
    children: Option<BoundedVec<WireId<NodeId>, AttackTreeNodesCap>>,
    #[serde(default, deserialize_with = "some")]
    parents: Option<BoundedVec<WireId<NodeId>, AttackTreeNodesCap>>,
}

/// An [`AttackTree`] decoded from its arena-order `nodes` array.
pub(super) struct NodesWire(pub(super) AttackTree);

impl<'de> Deserialize<'de> for NodesWire {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = BoundedVec::<NodeWire, AttackTreeNodesCap>::deserialize(d)?.0;
        let mut parts: Vec<WireNodeParts> = Vec::with_capacity(raw.len());
        for n in raw {
            let gate_children = match (n.gate, n.children) {
                (Some(g), Some(c)) => {
                    Some((GateKind::from(g), c.0.into_iter().map(|w| w.0).collect()))
                }
                (None, None) => None,
                _ => return Err(de::Error::custom("gate and children must appear together")),
            };
            let parents = n.parents.map(|p| p.0.into_iter().map(|w| w.0).collect());
            parts.push((n.id.0, n.label.0, gate_children, parents));
        }
        AttackTree::from_wire_parts(parts)
            .map(NodesWire)
            .ok_or_else(|| de::Error::custom("invalid attack tree"))
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

/// A visual ID formatted on the fly for [`Text`].
#[derive(Clone, Copy)]
pub(super) struct VisualDisplay {
    pub(super) prefix: VisualPrefix,
    pub(super) num: u32,
}

impl VisualDisplay {
    /// Returns `Some(Text(VisualDisplay { prefix, num }))` when `num`
    /// differs from the entity's 0-based position `pos` in canonical ID
    /// order, and `None` when `num == pos`. Time O(1).
    pub(super) fn non_default(
        prefix: VisualPrefix,
        num: Option<u32>,
        pos: usize,
    ) -> Option<Text<Self>> {
        let num = num?;
        if u32::try_from(pos).ok() == Some(num) {
            None
        } else {
            Some(Text(Self { prefix, num }))
        }
    }
}

impl fmt::Display for VisualDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{:02}", self.prefix.as_str(), self.num)
    }
}

/// Marker trait binding an entity wire type to its [`VisualPrefix`].
pub(super) trait VisualKind {
    const PREFIX: VisualPrefix;
}

macro_rules! visual_kinds {
    ($($name:ident = $prefix:expr;)*) => {$(
        pub(super) enum $name {}
        impl VisualKind for $name {
            const PREFIX: VisualPrefix = $prefix;
        }
    )*};
}

visual_kinds! {
    AssetVisual = VisualPrefix::Asset;
    DamageVisual = VisualPrefix::Damage;
    ThreatVisual = VisualPrefix::Threat;
    TreeVisual = VisualPrefix::Tree;
    ControlVisual = VisualPrefix::Control;
    GoalVisual = VisualPrefix::Goal;
    ClaimVisual = VisualPrefix::Claim;
    PartyVisual = VisualPrefix::Party;
    ConditionVisual = VisualPrefix::Condition;
    ComponentVisual = VisualPrefix::Component;
    InterfaceVisual = VisualPrefix::Interface;
    DataFlowVisual = VisualPrefix::DataFlow;
    TrustBoundaryVisual = VisualPrefix::TrustBoundary;
}

/// A visual ID decoded from its canonical text (`A00`, `T01`, `DF12`, ...).
/// Anything else is [`StoreError::InvalidId`]. Time O(1).
pub(super) struct WireVisual<K>(pub(super) u32, PhantomData<K>);

impl<'de, K: VisualKind> Deserialize<'de> for WireVisual<K> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        WithStr::new(|s: &str| {
            K::PREFIX
                .parse_num(s)
                .map(|n| WireVisual(n, PhantomData))
                .ok_or(Fault::InvalidId)
        })
        .deserialize(d)
    }
}

/// A format name: `aegis-project`, `aegis-oplog`, or
/// `aegis-methodology-pack`.
pub(crate) trait FormatName {
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
/// [`StoreError::UnsupportedFormat`] (or [`PackError::UnsupportedFormat`])
/// as soon as it is read.
pub(crate) struct Format<F>(PhantomData<F>);

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
pub(crate) struct FormatVersion;

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
    #[serde(default)]
    pub(super) attack_trees: Vec<TreeWire>,
    #[serde(default)]
    pub(super) security_controls: Vec<ControlWire>,
    #[serde(default)]
    pub(super) cybersecurity_goals: Vec<GoalWire>,
    #[serde(default)]
    pub(super) cybersecurity_claims: Vec<ClaimWire>,
    #[serde(default)]
    pub(super) parties: BoundedVec<PartyWire, PartiesCap>,
    #[serde(default)]
    pub(super) components: Vec<ComponentWire>,
    #[serde(default)]
    pub(super) interfaces: Vec<InterfaceWire>,
    #[serde(default)]
    pub(super) data_flows: Vec<DataFlowWire>,
    #[serde(default)]
    pub(super) trust_boundaries: Vec<TrustBoundaryWire>,
    #[serde(default)]
    pub(super) descriptions: Vec<DescriptionWire>,
    #[serde(default)]
    pub(super) review_marks: Vec<ReviewMarkWire>,
    #[serde(default)]
    pub(super) languages: Vec<LangText>,
    #[serde(default)]
    pub(super) elements: ElementsByKind,
    #[serde(default)]
    pub(super) conditions: BoundedVec<ConditionWire, ConditionsCap>,
    #[serde(default)]
    pub(super) condition_links: Vec<ConditionLinkWire>,
    #[serde(default)]
    pub(super) step_conditions: Vec<StepConditionWire>,
}

/// A snapshot condition (APL-01). `present` is written only when false.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConditionWire {
    pub(super) id: WireId<ConditionId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<ConditionVisual>>,
    pub(super) name: Name,
    #[serde(default, deserialize_with = "some")]
    pub(super) description: Option<DescriptionText>,
    #[serde(default = "present_default")]
    pub(super) present: bool,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

fn present_default() -> bool {
    true
}

/// The conditions one element needs, as public IDs.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConditionLinkWire {
    pub(super) target: TargetWire,
    pub(super) conditions: BoundedVec<WireId<ConditionId>, ConditionLinksCap>,
}

/// The conditions one attack step needs, as public IDs.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StepConditionWire {
    pub(super) tree: WireId<TreeId>,
    pub(super) node: WireId<NodeId>,
    pub(super) conditions: BoundedVec<WireId<ConditionId>, ConditionLinksCap>,
}

/// A snapshot element of a generic kind.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ElementWire {
    pub(super) id: WireId<ElementId>,
    pub(super) visual_id: Key,
    pub(super) name: Name,
    #[serde(default)]
    pub(super) attributes: BoundedKeyMap<AttrWire, ElementAttributesCap>,
    #[serde(default)]
    pub(super) links: BoundedVec<LinkWire, ElementLinksCap>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// The snapshot `elements` map: each kind key with its elements, in file
/// order. An unknown or repeated kind is refused.
#[derive(Default)]
pub(super) struct ElementsByKind(pub(super) Vec<(crate::element::ElementKind, Vec<ElementWire>)>);

impl<'de> Deserialize<'de> for ElementsByKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ElementsByKind;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut kinds: Vec<(crate::element::ElementKind, Vec<ElementWire>)> = Vec::new();
                while let Some(key) = map.next_key::<ElementKindKey>()? {
                    if kinds.iter().any(|(k, _)| *k == key.0) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    let list = map.next_value::<BoundedVec<ElementWire, ElementsCap>>()?;
                    kinds.push((key.0, list.0));
                }
                Ok(ElementsByKind(kinds))
            }
        }
        d.deserialize_map(V)
    }
}

/// A step whose rating awaits review.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewMarkWire {
    pub(super) tree: WireId<TreeId>,
    pub(super) node: WireId<NodeId>,
}

/// What a description belongs to, as written: `{"kind":"asset","id":...}`
/// or `{"kind":"project"}`.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TargetWire {
    Project,
    Asset { id: WireId<AssetId> },
    Damage { id: WireId<DamageId> },
    Threat { id: WireId<ThreatId> },
    Tree { id: WireId<TreeId> },
    Control { id: WireId<ControlId> },
    Goal { id: WireId<GoalId> },
    Claim { id: WireId<ClaimId> },
    Component { id: WireId<ComponentId> },
    Interface { id: WireId<InterfaceId> },
    DataFlow { id: WireId<DataFlowId> },
    TrustBoundary { id: WireId<TrustBoundaryId> },
    ItemDescription,
    ItemScope,
}

impl TargetWire {
    pub(super) fn into_ref(self) -> crate::description::ElementRef {
        use crate::description::ElementRef as E;
        match self {
            Self::Project => E::Project,
            Self::Asset { id } => E::Asset(id.0),
            Self::Damage { id } => E::Damage(id.0),
            Self::Threat { id } => E::Threat(id.0),
            Self::Tree { id } => E::Tree(id.0),
            Self::Control { id } => E::Control(id.0),
            Self::Goal { id } => E::Goal(id.0),
            Self::Claim { id } => E::Claim(id.0),
            Self::Component { id } => E::Component(id.0),
            Self::Interface { id } => E::Interface(id.0),
            Self::DataFlow { id } => E::DataFlow(id.0),
            Self::TrustBoundary { id } => E::TrustBoundary(id.0),
            Self::ItemDescription => E::ItemDescription,
            Self::ItemScope => E::ItemScope,
        }
    }
}

/// The writing side of [`TargetWire`].
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum TargetView {
    Project,
    Asset { id: Text<AssetId> },
    Damage { id: Text<DamageId> },
    Threat { id: Text<ThreatId> },
    Tree { id: Text<TreeId> },
    Control { id: Text<ControlId> },
    Goal { id: Text<GoalId> },
    Claim { id: Text<ClaimId> },
    Component { id: Text<ComponentId> },
    Interface { id: Text<InterfaceId> },
    DataFlow { id: Text<DataFlowId> },
    TrustBoundary { id: Text<TrustBoundaryId> },
    ItemDescription,
    ItemScope,
}

impl From<crate::description::ElementRef> for TargetView {
    fn from(target: crate::description::ElementRef) -> Self {
        use crate::description::ElementRef as E;
        match target {
            E::Project => Self::Project,
            E::Asset(id) => Self::Asset { id: Text(id) },
            E::Damage(id) => Self::Damage { id: Text(id) },
            E::Threat(id) => Self::Threat { id: Text(id) },
            E::Tree(id) => Self::Tree { id: Text(id) },
            E::Control(id) => Self::Control { id: Text(id) },
            E::Goal(id) => Self::Goal { id: Text(id) },
            E::Claim(id) => Self::Claim { id: Text(id) },
            E::Component(id) => Self::Component { id: Text(id) },
            E::Interface(id) => Self::Interface { id: Text(id) },
            E::DataFlow(id) => Self::DataFlow { id: Text(id) },
            E::TrustBoundary(id) => Self::TrustBoundary { id: Text(id) },
            E::ItemDescription => Self::ItemDescription,
            E::ItemScope => Self::ItemScope,
        }
    }
}

/// A snapshot description.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DescriptionWire {
    pub(super) target: TargetWire,
    pub(super) text: DescriptionText,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
    #[serde(default)]
    pub(super) translations: Vec<TranslationWire>,
}

/// One description translation, as read from a snapshot (FND-04).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TranslationWire {
    pub(super) lang: LangText,
    pub(super) text: DescriptionText,
}

/// A snapshot asset: 48 bytes plus its name.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssetWire {
    pub(super) id: WireId<AssetId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<AssetVisual>>,
    pub(super) name: Name,
    pub(super) properties: PropertySetKeys,
    #[serde(default, deserialize_with = "some")]
    pub(super) component: Option<WireId<ComponentId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot damage scenario; unrated categories are absent.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DamageWire {
    pub(super) id: WireId<DamageId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<DamageVisual>>,
    pub(super) name: Name,
    pub(super) impacts: Impacts,
    #[serde(default, deserialize_with = "some")]
    pub(super) impact_rationale: Option<DescriptionText>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// One damage link in the object form of a threat's `damages`: a public ID
/// and its optional rationale. The old string-array form has no rationale.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireDamageLink {
    pub(super) damage: WireId<DamageId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) rationale: Option<DescriptionText>,
}

/// A threat's damage links: either the v1 string array of IDs, or the object
/// array used only when at least one link carries a rationale. Both variants
/// keep the same [`LinksCap`] cap.
#[derive(serde::Deserialize)]
#[serde(untagged)]
pub(super) enum DamageLinksWire {
    Ids(BoundedVec<WireId<DamageId>, LinksCap>),
    Links(BoundedVec<WireDamageLink, LinksCap>),
}

/// A snapshot threat scenario.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ThreatWire {
    pub(super) id: WireId<ThreatId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<ThreatVisual>>,
    pub(super) name: Name,
    pub(super) asset: WireId<AssetId>,
    pub(super) category: ThreatCategoryKey,
    pub(super) catalog_refs: BoundedVec<CatalogRefText, RefsCap>,
    pub(super) damages: DamageLinksWire,
    #[serde(default, deserialize_with = "some")]
    pub(super) direct_feasibility: Option<FeasibilityLevelKey>,
    #[serde(default, deserialize_with = "some")]
    pub(super) attack_tree: Option<WireId<TreeId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) treatment: Option<RiskTreatmentWire>,
    #[serde(default, deserialize_with = "some")]
    pub(super) goal: Option<WireId<GoalId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) claim: Option<WireId<ClaimId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) scales: Option<bool>,
    #[serde(default, deserialize_with = "some")]
    pub(super) scaling_reason: Option<DescriptionText>,
    #[serde(default, deserialize_with = "some")]
    pub(super) feasibility_rationale: Option<DescriptionText>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot attack tree.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TreeWire {
    pub(super) id: WireId<TreeId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<TreeVisual>>,
    pub(super) nodes: NodesWire,
    pub(super) step_feasibility: StepFeasibility,
    #[serde(default)]
    pub(super) step_factors: StepFactors,
    #[serde(default)]
    pub(super) step_mitigations: StepMitigations,
    // Boxed: a snapshot holds every `TreeWire` at once, and most have none.
    #[serde(default, deserialize_with = "some")]
    pub(super) step_via: Option<Box<StepValues<ItemWire>>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) step_attack_type: Option<Box<StepValues<AttackTypeKey>>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// Applied targets of a security control in a snapshot: at most 64
/// entries ([`MAX_CONTROL_TARGETS`]), no target repeated, never empty
/// (omit `targets` instead).
#[derive(Default)]
pub(super) struct ControlTargetsWire(pub(super) Vec<ItemRef>);

impl<'de> Deserialize<'de> for ControlTargetsWire {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ControlTargetsWire;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list of control target items")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<ControlTargetsWire, A::Error> {
                let mut out: Vec<ItemRef> = Vec::new();
                while let Some(item) = seq.next_element::<ItemWire>()? {
                    if out.len() >= MAX_CONTROL_TARGETS {
                        return Err(raise(Fault::Limit(Limit::CONTROL_TARGETS)));
                    }
                    let target = item.unwrap_item();
                    if out.contains(&target) {
                        return Err(raise(Fault::Duplicate));
                    }
                    out.push(target);
                }
                if out.is_empty() {
                    return Err(de::Error::custom(
                        "control targets must be omitted when empty",
                    ));
                }
                Ok(ControlTargetsWire(out))
            }
        }
        d.deserialize_seq(V)
    }
}

/// Control rules given as an object from attack type key to feasibility
/// level key: at most 16 entries ([`MAX_CONTROL_RULES`]), no attack type
/// repeated, each pair resolved as it is read.
#[derive(Default)]
pub(super) struct ControlRulesWire(pub(super) Vec<(AttackType, FeasibilityLevel)>);

impl<'de> Deserialize<'de> for ControlRulesWire {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ControlRulesWire;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object of control rules")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<ControlRulesWire, A::Error> {
                let m = context().map_err(raise)?;
                let mut out = Vec::new();
                let mut seen = 0u64;
                let mut count = 0usize;
                let mut pending = None;
                while let Some(attack_type) = map.next_key_seed(WithStr::new(|s: &str| {
                    check_key_len(s)?;
                    Ok(note_unknown(
                        m.attack_type(s),
                        VocabKind::AttackType,
                        s,
                        &mut pending,
                    ))
                }))? {
                    count += 1;
                    if count > MAX_CONTROL_RULES {
                        return Err(raise(Fault::Limit(Limit::CONTROL_RULES)));
                    }
                    if let Some(t) = attack_type {
                        let bit = u32::try_from(t.index())
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
                        Ok(note_unknown(
                            m.feasibility_level(s),
                            VocabKind::FeasibilityLevel,
                            s,
                            &mut pending,
                        ))
                    }))?;
                    if let (Some(t), Some(l)) = (attack_type, level) {
                        out.push((t, l));
                    }
                }
                match pending {
                    Some(fault) => Err(raise(fault)),
                    None if count == 0 => Err(de::Error::custom(
                        "control rules must be omitted when empty",
                    )),
                    None => Ok(ControlRulesWire(out)),
                }
            }
        }
        d.deserialize_map(V)
    }
}

/// A snapshot security control.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ControlWire {
    pub(super) id: WireId<ControlId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<ControlVisual>>,
    pub(super) name: Name,
    #[serde(default, deserialize_with = "some")]
    pub(super) kind: Option<ControlKindKey>,
    #[serde(default)]
    pub(super) targets: ControlTargetsWire,
    #[serde(default)]
    pub(super) rules: ControlRulesWire,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot cybersecurity goal.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GoalWire {
    pub(super) id: WireId<GoalId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<GoalVisual>>,
    pub(super) name: Name,
    pub(super) controls: BoundedVec<WireId<ControlId>, GoalControlsCap>,
    #[serde(default)]
    pub(super) responsible: BoundedVec<WireId<PartyId>, ResponsibleCap>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot cybersecurity claim.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClaimWire {
    pub(super) id: WireId<ClaimId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<ClaimVisual>>,
    pub(super) name: Name,
    #[serde(default)]
    pub(super) responsible: BoundedVec<WireId<PartyId>, ResponsibleCap>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot party.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PartyWire {
    pub(super) id: WireId<PartyId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<PartyVisual>>,
    pub(super) name: Name,
    pub(super) kind: crate::party::PartyKind,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot item-definition component.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ComponentWire {
    pub(super) id: WireId<ComponentId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<ComponentVisual>>,
    pub(super) name: Name,
    #[serde(default, deserialize_with = "some")]
    pub(super) parent: Option<WireId<ComponentId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) system: Option<WireId<ElementId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot item-definition interface.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InterfaceWire {
    pub(super) id: WireId<InterfaceId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<InterfaceVisual>>,
    pub(super) name: Name,
    pub(super) component: WireId<ComponentId>,
    pub(super) kind: InterfaceKindKey,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot item-definition data flow.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DataFlowWire {
    pub(super) id: WireId<DataFlowId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<DataFlowVisual>>,
    pub(super) name: Name,
    pub(super) from: WireId<ComponentId>,
    pub(super) to: WireId<ComponentId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) interface: Option<WireId<InterfaceId>>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
}

/// A snapshot item-definition trust boundary.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TrustBoundaryWire {
    pub(super) id: WireId<TrustBoundaryId>,
    #[serde(default, deserialize_with = "some")]
    pub(super) visual_id: Option<WireVisual<TrustBoundaryVisual>>,
    pub(super) name: Name,
    pub(super) components: BoundedVec<WireId<ComponentId>, BoundaryMembersCap>,
    #[serde(default, deserialize_with = "some")]
    pub(super) provenance: Option<ProvenanceWire>,
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
    SetReviewMark {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        marked: bool,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddTreeNode {
        tree: WireId<TreeId>,
        parent: WireId<NodeId>,
        id: WireId<NodeId>,
        label: Name,
        #[serde(default, deserialize_with = "some")]
        gate: Option<GateKindWire>,
        index: u32,
        position: u32,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveTreeNode {
        tree: WireId<TreeId>,
        id: WireId<NodeId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    LinkTreeNode {
        tree: WireId<TreeId>,
        parent: WireId<NodeId>,
        child: WireId<NodeId>,
        position: u32,
        slot: u32,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnlinkTreeNode {
        tree: WireId<TreeId>,
        parent: WireId<NodeId>,
        child: WireId<NodeId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameTreeNode {
        tree: WireId<TreeId>,
        id: WireId<NodeId>,
        label: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetTreeNodeGate {
        tree: WireId<TreeId>,
        id: WireId<NodeId>,
        gate: GateKindWire,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetDescription {
        target: TargetWire,
        text: DescriptionText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetDescriptionTranslation {
        target: TargetWire,
        lang: LangText,
        text: DescriptionText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetProjectLanguages {
        langs: Vec<LangText>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddAsset {
        id: WireId<AssetId>,
        name: Name,
        properties: PropertySetKeys,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameAsset {
        id: WireId<AssetId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetAssetProperties {
        id: WireId<AssetId>,
        properties: PropertySetKeys,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveAsset {
        id: WireId<AssetId>,
    },
    AddDamageScenario {
        id: WireId<DamageId>,
        name: Name,
        impacts: Impacts,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameDamageScenario {
        id: WireId<DamageId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetImpact {
        id: WireId<DamageId>,
        category: ImpactCategoryKey,
        // `deserialize_with` makes the field required: `null` is unrated,
        // a missing field is an error.
        #[serde(deserialize_with = "Option::deserialize")]
        level: Option<Key>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
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
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameThreatScenario {
        id: WireId<ThreatId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetThreatCategory {
        id: WireId<ThreatId>,
        category: ThreatCategoryKey,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddCatalogRef {
        threat: WireId<ThreatId>,
        catalog_ref: CatalogRefText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveCatalogRef {
        threat: WireId<ThreatId>,
        catalog_ref: CatalogRefText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    LinkDamage {
        threat: WireId<ThreatId>,
        damage: WireId<DamageId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnlinkDamage {
        threat: WireId<ThreatId>,
        damage: WireId<DamageId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveThreatScenario {
        id: WireId<ThreatId>,
    },
    AddAttackTree {
        id: WireId<TreeId>,
        nodes: NodesWire,
        step_feasibility: StepFeasibility,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetStepFeasibility {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        #[serde(deserialize_with = "Option::deserialize")]
        level: Option<FeasibilityLevelKey>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetStepVia {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        #[serde(deserialize_with = "Option::deserialize")]
        via: Option<ItemWire>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetStepAttackType {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        #[serde(deserialize_with = "Option::deserialize")]
        attack_type: Option<AttackTypeKey>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetStepFactor {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        factor: FeasibilityFactorKey,
        #[serde(deserialize_with = "Option::deserialize")]
        level: Option<Key>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetThreatFeasibility {
        threat: WireId<ThreatId>,
        #[serde(deserialize_with = "Option::deserialize")]
        level: Option<FeasibilityLevelKey>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetThreatAttackTree {
        threat: WireId<ThreatId>,
        #[serde(deserialize_with = "Option::deserialize")]
        tree: Option<WireId<TreeId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveAttackTree {
        id: WireId<TreeId>,
    },
    SetRiskTreatment {
        threat: WireId<ThreatId>,
        #[serde(deserialize_with = "Option::deserialize")]
        treatment: Option<RiskTreatmentWire>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddSecurityControl {
        id: WireId<ControlId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameSecurityControl {
        id: WireId<ControlId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveSecurityControl {
        id: WireId<ControlId>,
    },
    ApplyControl {
        control: WireId<ControlId>,
        target: ItemWire,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnapplyControl {
        control: WireId<ControlId>,
        target: ItemWire,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetControlRule {
        control: WireId<ControlId>,
        attack_type: AttackTypeKey,
        #[serde(deserialize_with = "Option::deserialize")]
        ceiling: Option<FeasibilityLevelKey>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetControlKind {
        control: WireId<ControlId>,
        #[serde(deserialize_with = "Option::deserialize")]
        kind: Option<ControlKindKey>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetStepMitigation {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        control: WireId<ControlId>,
        #[serde(deserialize_with = "Option::deserialize")]
        feasibility: Option<FeasibilityLevelKey>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddCybersecurityGoal {
        id: WireId<GoalId>,
        name: Name,
        controls: BoundedVec<WireId<ControlId>, GoalControlsCap>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameCybersecurityGoal {
        id: WireId<GoalId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    LinkGoalControl {
        goal: WireId<GoalId>,
        control: WireId<ControlId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnlinkGoalControl {
        goal: WireId<GoalId>,
        control: WireId<ControlId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    LinkGoalParty {
        goal: WireId<GoalId>,
        party: WireId<PartyId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnlinkGoalParty {
        goal: WireId<GoalId>,
        party: WireId<PartyId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveCybersecurityGoal {
        id: WireId<GoalId>,
    },
    AddCybersecurityClaim {
        id: WireId<ClaimId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameCybersecurityClaim {
        id: WireId<ClaimId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    LinkClaimParty {
        claim: WireId<ClaimId>,
        party: WireId<PartyId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnlinkClaimParty {
        claim: WireId<ClaimId>,
        party: WireId<PartyId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveCybersecurityClaim {
        id: WireId<ClaimId>,
    },
    AddParty {
        id: WireId<PartyId>,
        name: Name,
        kind: crate::party::PartyKind,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameParty {
        id: WireId<PartyId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetPartyKind {
        id: WireId<PartyId>,
        kind: crate::party::PartyKind,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveParty {
        id: WireId<PartyId>,
    },
    AddCondition {
        id: WireId<ConditionId>,
        name: Name,
        description: DescriptionText,
        present: bool,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameCondition {
        id: WireId<ConditionId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetConditionPresent {
        id: WireId<ConditionId>,
        present: bool,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveCondition {
        id: WireId<ConditionId>,
    },
    NeedCondition {
        element: TargetWire,
        condition: WireId<ConditionId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnneedCondition {
        element: TargetWire,
        condition: WireId<ConditionId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    NeedStepCondition {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        condition: WireId<ConditionId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnneedStepCondition {
        tree: WireId<TreeId>,
        node: WireId<NodeId>,
        condition: WireId<ConditionId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetThreatGoal {
        threat: WireId<ThreatId>,
        #[serde(deserialize_with = "Option::deserialize")]
        goal: Option<WireId<GoalId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetThreatClaim {
        threat: WireId<ThreatId>,
        #[serde(deserialize_with = "Option::deserialize")]
        claim: Option<WireId<ClaimId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetThreatScaling {
        threat: WireId<ThreatId>,
        #[serde(deserialize_with = "Option::deserialize")]
        scales: Option<bool>,
        reason: DescriptionText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetImpactRationale {
        damage: WireId<DamageId>,
        text: DescriptionText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetDamageLinkRationale {
        threat: WireId<ThreatId>,
        damage: WireId<DamageId>,
        text: DescriptionText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetFeasibilityRationale {
        threat: WireId<ThreatId>,
        text: DescriptionText,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddComponent {
        id: WireId<ComponentId>,
        name: Name,
        #[serde(deserialize_with = "Option::deserialize")]
        parent: Option<WireId<ComponentId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameComponent {
        id: WireId<ComponentId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetComponentParent {
        id: WireId<ComponentId>,
        #[serde(deserialize_with = "Option::deserialize")]
        parent: Option<WireId<ComponentId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetComponentSystem {
        id: WireId<ComponentId>,
        #[serde(deserialize_with = "Option::deserialize")]
        system: Option<WireId<ElementId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveComponent {
        id: WireId<ComponentId>,
    },
    AddInterface {
        id: WireId<InterfaceId>,
        name: Name,
        component: WireId<ComponentId>,
        kind: InterfaceKindKey,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameInterface {
        id: WireId<InterfaceId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetInterfaceKind {
        id: WireId<InterfaceId>,
        kind: InterfaceKindKey,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveInterface {
        id: WireId<InterfaceId>,
    },
    AddDataFlow {
        id: WireId<DataFlowId>,
        name: Name,
        from: WireId<ComponentId>,
        to: WireId<ComponentId>,
        #[serde(deserialize_with = "Option::deserialize")]
        interface: Option<WireId<InterfaceId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameDataFlow {
        id: WireId<DataFlowId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    SetDataFlowInterface {
        id: WireId<DataFlowId>,
        #[serde(deserialize_with = "Option::deserialize")]
        interface: Option<WireId<InterfaceId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveDataFlow {
        id: WireId<DataFlowId>,
    },
    AddTrustBoundary {
        id: WireId<TrustBoundaryId>,
        name: Name,
        components: BoundedVec<WireId<ComponentId>, BoundaryMembersCap>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameTrustBoundary {
        id: WireId<TrustBoundaryId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddBoundaryComponent {
        boundary: WireId<TrustBoundaryId>,
        component: WireId<ComponentId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveBoundaryComponent {
        boundary: WireId<TrustBoundaryId>,
        component: WireId<ComponentId>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveTrustBoundary {
        id: WireId<TrustBoundaryId>,
    },
    SetAssetComponent {
        asset: WireId<AssetId>,
        #[serde(deserialize_with = "Option::deserialize")]
        component: Option<WireId<ComponentId>>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    AddElement {
        id: WireId<ElementId>,
        kind: ElementKindKey,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RenameElement {
        id: WireId<ElementId>,
        name: Name,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    RemoveElement {
        id: WireId<ElementId>,
    },
    SetElementAttribute {
        id: WireId<ElementId>,
        key: Key,
        #[serde(deserialize_with = "Option::deserialize")]
        value: Option<AttrWire>,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    LinkElement {
        id: WireId<ElementId>,
        target: LinkWire,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
    UnlinkElement {
        id: WireId<ElementId>,
        target: LinkWire,
        #[serde(default, deserialize_with = "some")]
        provenance: Option<ProvenanceWire>,
    },
}

/// One element link target on the wire.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum LinkWire {
    Threat(WireId<ThreatId>),
}

impl LinkWire {
    pub(super) fn into_target(self) -> crate::element::LinkTarget {
        match self {
            Self::Threat(t) => crate::element::LinkTarget::Threat(t.0),
        }
    }
}

/// One element attribute value on the wire.
#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AttrWire {
    Level(Key),
    Choice(Key),
    Text(Name),
}

impl AttrWire {
    pub(super) fn into_value(self) -> crate::element::AttrValue {
        use crate::element::AttrValue as V;
        match self {
            Self::Level(k) => V::Level(k.0),
            Self::Choice(k) => V::Choice(k.0),
            Self::Text(t) => V::Text(t.0),
        }
    }
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

/// Resolves the `SetStepFactor` level key within its factor. Time O(log m).
pub(super) fn resolve_factor_level(
    m: &Methodology,
    factor: FeasibilityFactor,
    key: &Key,
) -> Result<FactorLevel, StoreError> {
    m.factor_level(factor, &key.0)
        .ok_or_else(|| StoreError::UnknownKey {
            kind: VocabKind::FactorLevel,
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
    /// Feasibility factors sorted by key.
    feasibility_factors: Vec<FeasibilityFactor>,
    /// Attack types sorted by key.
    attack_types: Vec<AttackType>,
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
        let mut feasibility_factors: Vec<FeasibilityFactor> = m.feasibility_factors().collect();
        feasibility_factors.sort_by_key(|&f| m.feasibility_factor_key(f));
        let mut attack_types: Vec<AttackType> = m.attack_types().collect();
        attack_types.sort_by_key(|&t| m.attack_type_key(t));
        KeyOrder {
            property_rank,
            properties,
            impact_categories,
            catalog_schemes,
            feasibility_factors,
            attack_types,
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

/// Non-default provenance as `{"actor": ..., "timestamp_ms": ..., "source": ...}`.
#[derive(serde::Serialize)]
pub(super) struct ProvenanceView<'a> {
    actor: &'a str,
    timestamp_ms: u64,
    source: ProvenanceSourceWire,
}

impl<'a> ProvenanceView<'a> {
    pub(super) fn of(p: &'a Provenance) -> Option<Self> {
        if p.is_default() {
            None
        } else {
            Some(Self {
                actor: p.actor(),
                timestamp_ms: p.timestamp_ms(),
                source: p.source().into(),
            })
        }
    }
}

/// Step feasibility ratings in the given order (ops), as an object. Fails
/// on a repeated node ID or a level not in the methodology. O(k).
pub(super) struct StepFeasibilityInOrder<'a> {
    pub(super) m: &'a Methodology,
    pub(super) ratings: &'a [(NodeId, FeasibilityLevel)],
}

impl Serialize for StepFeasibilityInOrder<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.ratings.len()))?;
        let mut seen: Vec<NodeId> = Vec::with_capacity(self.ratings.len());
        for &(node, level) in self.ratings {
            let Some(lk) = self.m.feasibility_level_key(level) else {
                return Err(ser::Error::custom(
                    "feasibility level not in the methodology",
                ));
            };
            if seen.contains(&node) {
                return Err(ser::Error::custom("step feasibility node repeated"));
            }
            seen.push(node);
            map.serialize_entry(&Text(node), lk)?;
        }
        map.end()
    }
}

/// A stored attack tree's step feasibility ratings in arena order, unrated
/// steps left out, as an object. O(n).
pub(super) struct StepFeasibilityByNode<'a> {
    pub(super) project: &'a crate::project::Project,
    pub(super) tree_idx: TreeIdx,
    pub(super) tree: &'a AttackTree,
}

impl Serialize for StepFeasibilityByNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let m = self.project.methodology();
        let mut map = s.serialize_map(None)?;
        for (node_idx, node_id, _) in self.tree.iter() {
            if let Some(level) = self.project.step_feasibility(self.tree_idx, node_idx) {
                map.serialize_entry(&Text(node_id), &Required(m.feasibility_level_key(level)))?;
            }
        }
        map.end()
    }
}

/// A stored attack tree's step factor ratings in arena order, steps without
/// factor ratings left out, as an object mapping node ID to `{factor_key:
/// factor_level_key}` in key order. O(V * F).
pub(super) struct StepFactorsByNode<'a> {
    pub(super) project: &'a crate::project::Project,
    pub(super) order: &'a KeyOrder,
    pub(super) tree_idx: TreeIdx,
    pub(super) tree: &'a AttackTree,
}

impl StepFactorsByNode<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.tree.iter().all(|(node_idx, _, _)| {
            self.project
                .step_factor_slice(self.tree_idx, node_idx)
                .is_empty()
        })
    }
}

impl Serialize for StepFactorsByNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        for (node_idx, node_id, _) in self.tree.iter() {
            let factors = self.project.step_factor_slice(self.tree_idx, node_idx);
            if !factors.is_empty() {
                map.serialize_entry(
                    &Text(node_id),
                    &StepFactorsForNode {
                        m: self.project.methodology(),
                        order: self.order,
                        factors,
                    },
                )?;
            }
        }
        map.end()
    }
}

struct StepFactorsForNode<'a> {
    m: &'a Methodology,
    order: &'a KeyOrder,
    factors: &'a [Option<FactorLevel>],
}

impl Serialize for StepFactorsForNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let rated = self.factors.iter().filter(|l| l.is_some()).count();
        let mut map = s.serialize_map(Some(rated))?;
        let mut written = 0usize;
        for &f in &self.order.feasibility_factors {
            if let Some(&Some(l)) = self.factors.get(f.index()) {
                map.serialize_entry(
                    &Required(self.m.feasibility_factor_key(f)),
                    &Required(self.m.factor_level_key(f, l)),
                )?;
                written += 1;
            }
        }
        if written != rated {
            return Err(ser::Error::custom("factor rating not in the methodology"));
        }
        map.end()
    }
}

/// A stored attack tree's step mitigations in arena order, steps without
/// mitigations left out, as an object mapping node ID to `{control_id:
/// feasibility_key}`. O(V + M).
pub(super) struct StepMitigationsByNode<'a> {
    pub(super) project: &'a crate::project::Project,
    pub(super) tree_idx: TreeIdx,
    pub(super) tree: &'a AttackTree,
}

impl StepMitigationsByNode<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.tree.iter().all(|(node_idx, _, _)| {
            self.project
                .step_mitigations(self.tree_idx, node_idx)
                .is_empty()
        })
    }
}

impl Serialize for StepMitigationsByNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        for (node_idx, node_id, _) in self.tree.iter() {
            let mitigations = self.project.step_mitigations(self.tree_idx, node_idx);
            if !mitigations.is_empty() {
                map.serialize_entry(
                    &Text(node_id),
                    &StepMitigationsForNode {
                        project: self.project,
                        mitigations,
                    },
                )?;
            }
        }
        map.end()
    }
}

struct StepMitigationsForNode<'a> {
    project: &'a crate::project::Project,
    mitigations: &'a [StepMitigation],
}

impl Serialize for StepMitigationsForNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let m = self.project.methodology();
        let mut map = s.serialize_map(Some(self.mitigations.len()))?;
        for mt in self.mitigations {
            map.serialize_entry(
                &Required(self.project.security_control_id(mt.control()).map(Text)),
                &Required(m.feasibility_level_key(mt.feasibility())),
            )?;
        }
        map.end()
    }
}

/// A stored attack tree's step "through" items in arena order, steps
/// without one left out, as an object mapping node ID to `{"component":
/// id}` or `{"interface": id}`. O(V).
pub(super) struct StepViaByNode<'a> {
    pub(super) project: &'a crate::project::Project,
    pub(super) tree_idx: TreeIdx,
    pub(super) tree: &'a AttackTree,
}

impl StepViaByNode<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.tree
            .iter()
            .all(|(n, _, _)| self.project.step_via(self.tree_idx, n).is_none())
    }
}

impl Serialize for StepViaByNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        for (node_idx, node_id, _) in self.tree.iter() {
            if let Some(item) = self.project.step_via(self.tree_idx, node_idx) {
                map.serialize_entry(&Text(node_id), &ItemView::of(item))?;
            }
        }
        map.end()
    }
}

/// A stored attack tree's step attack types in arena order, steps without
/// one left out, as an object mapping node ID to the attack type key. O(V).
pub(super) struct StepAttackTypeByNode<'a> {
    pub(super) project: &'a crate::project::Project,
    pub(super) tree_idx: TreeIdx,
    pub(super) tree: &'a AttackTree,
}

impl StepAttackTypeByNode<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.tree
            .iter()
            .all(|(n, _, _)| self.project.step_attack_type(self.tree_idx, n).is_none())
    }
}

impl Serialize for StepAttackTypeByNode<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let m = self.project.methodology();
        let mut map = s.serialize_map(None)?;
        for (node_idx, node_id, _) in self.tree.iter() {
            if let Some(t) = self.project.step_attack_type(self.tree_idx, node_idx) {
                map.serialize_entry(&Text(node_id), &Required(m.attack_type_key(t)))?;
            }
        }
        map.end()
    }
}

/// A stored security control's applied targets in canonical `ItemRef`
/// order (`Component` before `Interface`, then by ID). O(T).
pub(super) struct ControlTargetsByItem<'a> {
    pub(super) project: &'a crate::project::Project,
    pub(super) control: &'a crate::control::SecurityControl,
}

impl ControlTargetsByItem<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.control.targets.is_empty()
    }
}

impl Serialize for ControlTargetsByItem<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.control
                .targets
                .iter()
                .map(|&t| Required(self.project.item_ref(Some(t)).map(ItemView::of))),
        )
    }
}

/// A stored security control's rules in attack-type key order, as an object
/// mapping attack-type key to feasibility-level key. O(A + R).
pub(super) struct ControlRulesByKey<'a> {
    pub(super) m: &'a Methodology,
    pub(super) order: &'a KeyOrder,
    pub(super) control: &'a crate::control::SecurityControl,
}

impl ControlRulesByKey<'_> {
    pub(super) fn is_empty(&self) -> bool {
        self.control.rules().is_empty()
    }
}

impl Serialize for ControlRulesByKey<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let rules = self.control.rules();
        let mut map = s.serialize_map(Some(rules.len()))?;
        let mut written = 0usize;
        for &t in &self.order.attack_types {
            if let Some(level) = self.control.rule_for(t) {
                map.serialize_entry(
                    &Required(self.m.attack_type_key(t)),
                    &Required(self.m.feasibility_level_key(level)),
                )?;
                written += 1;
            }
        }
        if written != rules.len() {
            return Err(ser::Error::custom("control rule not in the methodology"));
        }
        map.end()
    }
}

/// An [`AttackTree`]'s nodes in arena order. O(V + E).
pub(super) struct NodesView<'a>(pub(super) &'a AttackTree);

impl Serialize for NodesView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|(idx, id, node)| NodeView {
            tree: self.0,
            idx,
            id,
            node,
        }))
    }
}

struct NodeView<'a> {
    tree: &'a AttackTree,
    idx: NodeIdx,
    id: NodeId,
    node: &'a Node,
}

impl Serialize for NodeView<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let gate = self.node.gate_kind();
        let has_parents = !self.tree.linked_parents_of(self.idx).is_empty();
        let len = 2 + usize::from(gate.is_some()) * 2 + usize::from(has_parents);
        let mut st = s.serialize_struct("Node", len)?;
        st.serialize_field("id", &Text(self.id))?;
        st.serialize_field("label", self.node.label())?;
        if let Some(kind) = gate {
            st.serialize_field("gate", &GateKindWire::from(kind))?;
            st.serialize_field(
                "children",
                &NodeIdxSliceIds {
                    tree: self.tree,
                    indices: self.node.children(),
                },
            )?;
        }
        if has_parents {
            st.serialize_field(
                "parents",
                &NodeParentIds {
                    tree: self.tree,
                    idx: self.idx,
                },
            )?;
        }
        st.end()
    }
}

struct NodeIdxSliceIds<'a> {
    tree: &'a AttackTree,
    indices: &'a [NodeIdx],
}

impl Serialize for NodeIdxSliceIds<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.indices
                .iter()
                .map(|&idx| Required(self.tree.id_of(idx).map(Text))),
        )
    }
}

struct NodeParentIds<'a> {
    tree: &'a AttackTree,
    idx: NodeIdx,
}

impl Serialize for NodeParentIds<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.tree
                .parents(self.idx)
                .map(|idx| Required(self.tree.id_of(idx).map(Text))),
        )
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
    fn decode_pack_maps_faults() {
        let with = wire_decode_pack_with;
        assert!(matches!(
            with(Some(Fault::Limit(Limit::LABEL))),
            Err(PackError::LimitExceeded(Limit::LabelBytes { max: 1024 }))
        ));
        assert!(matches!(
            with(Some(Fault::UnsupportedFormat(Some(3)))),
            Err(PackError::UnsupportedFormat { version: Some(3) })
        ));
        // Store-only faults never come from the pack wire types; if one
        // did, it would be a shape error like a plain serde error.
        assert!(matches!(
            with(Some(Fault::Duplicate)),
            Err(PackError::Decode { .. })
        ));
        assert!(matches!(with(None), Err(PackError::Decode { .. })));
        // The cell is clean for the next decode.
        assert_eq!(FAULT.with(Cell::take), None);
    }

    /// Runs `decode_pack` over a closure that fails after recording `fault`.
    fn wire_decode_pack_with(fault: Option<Fault>) -> Result<(), PackError> {
        decode_pack(|| {
            let mut d = serde_json::Deserializer::from_str("\"x\"");
            let _ = de::IgnoredAny::deserialize(&mut d)?;
            Err(match fault {
                Some(f) => raise::<serde_json::Error>(f),
                None => de::Error::custom("shape"),
            })
        })
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
        let key = testkit::second_def().key;
        let ok = de::<CheckedMethodology>(&format!(r#"{{"key":"{key}","version":"9"}}"#));
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
        let factors: Vec<_> = order
            .feasibility_factors
            .iter()
            .filter_map(|&f| m.feasibility_factor_key(f))
            .collect();
        assert_eq!(factors, ["exposure", "resources", "skill"]);
    }

    #[test]
    fn step_factors_decode_and_reject_invalid() {
        let m = testkit::second();
        let n1 = testkit::node_id(1);
        let exp = testkit::factor(&m, "exposure");
        let local = testkit::factor_lvl(&m, exp, "local");
        let ok = format!(r#"{{"{n1}":{{"exposure":"local"}}}}"#);
        assert_eq!(
            de::<StepFactors>(&ok).map(|s| s.0).ok(),
            Some(vec![(n1, vec![(exp, local)])])
        );

        // Empty outer map is rejected.
        assert!(matches!(
            de::<StepFactors>("{}").err(),
            Some(StoreError::Decode { .. })
        ));
        // Empty inner map is rejected.
        assert!(matches!(
            de::<StepFactors>(&format!(r#"{{"{n1}":{{}}}}"#)).err(),
            Some(StoreError::Decode { .. })
        ));
        // Duplicate factor key is rejected.
        assert!(matches!(
            de::<StepFactors>(&format!(
                r#"{{"{n1}":{{"exposure":"local","exposure":"open"}}}}"#
            ))
            .err(),
            Some(StoreError::DuplicateEntry)
        ));
        // Unknown factor key.
        assert!(matches!(
            de::<StepFactors>(&format!(r#"{{"{n1}":{{"nope":"local"}}}}"#)).err(),
            Some(StoreError::UnknownKey {
                kind: VocabKind::FeasibilityFactor,
                key,
            }) if &*key == "nope"
        ));
        // Unknown factor level key.
        assert!(matches!(
            de::<StepFactors>(&format!(r#"{{"{n1}":{{"exposure":"nope"}}}}"#)).err(),
            Some(StoreError::UnknownKey {
                kind: VocabKind::FactorLevel,
                key,
            }) if &*key == "nope"
        ));
    }
}
