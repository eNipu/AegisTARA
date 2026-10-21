//! The project: assets, damage scenarios, and threat scenarios over one
//! methodology, changed only through typed [`Op`]s.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::asset::{Asset, PropertySet};
use crate::damage::DamageScenario;
use crate::error::ModelError;
use crate::id::{ArenaIndex, AssetId, AssetIdx, DamageId, DamageIdx, ThreatId, ThreatIdx};
use crate::methodology::{
    CatalogRef, ImpactCategory, Level, Methodology, Property, ThreatCategory,
};
use crate::op::Op;
use crate::threat::{MAX_CATALOG_REFS, ThreatScenario};

/// Longest entity name, in chars.
pub const MAX_NAME_CHARS: usize = 256;

// ---------------------------------------------------------------------------
// Arena.
// ---------------------------------------------------------------------------

/// One entity kind: slots indexed by `Idx`, a parallel `Vec` of public IDs,
/// and a `BTreeMap` from ID to index (iteration order reaches output, D7).
/// A removed slot becomes `None` (a tombstone) and is never reused, so an
/// index never changes meaning while the project lives.
#[derive(Clone, Debug)]
struct Arena<Id, Idx, T> {
    slots: Vec<Option<T>>,
    ids: Vec<Id>,
    by_id: BTreeMap<Id, Idx>,
}

impl<Id: Copy + Ord, Idx: ArenaIndex, T> Arena<Id, Idx, T> {
    fn new() -> Self {
        Arena {
            slots: Vec::new(),
            ids: Vec::new(),
            by_id: BTreeMap::new(),
        }
    }

    /// The live entity at `idx`. O(1).
    fn get(&self, idx: Idx) -> Option<&T> {
        self.slots.get(idx.position()).and_then(Option::as_ref)
    }

    /// The live entity at `idx`, mutably. O(1).
    fn get_mut(&mut self, idx: Idx) -> Option<&mut T> {
        self.slots.get_mut(idx.position()).and_then(Option::as_mut)
    }

    /// The index of a live entity. O(log n).
    fn idx(&self, id: &Id) -> Option<Idx> {
        self.by_id.get(id).copied()
    }

    /// The public ID of a live entity. O(1).
    fn id(&self, idx: Idx) -> Option<Id> {
        self.get(idx)?;
        self.ids.get(idx.position()).copied()
    }

    /// Live entities. O(1).
    fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Slots, tombstones included. O(1).
    fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// The index the next push will use, if the arena has room under
    /// `limit` slots. O(1).
    fn next_idx(&self, limit: u32) -> Option<Idx> {
        u32::try_from(self.slots.len())
            .ok()
            .filter(|&i| i < limit)
            .map(Idx::from_u32)
    }

    /// Appends a live entity. Returns `None`, changing nothing, if the
    /// arena is full. O(log n) amortized.
    fn push(&mut self, id: Id, value: T, limit: u32) -> Option<Idx> {
        let idx = self.next_idx(limit)?;
        self.slots.push(Some(value));
        self.ids.push(id);
        self.by_id.insert(id, idx);
        Some(idx)
    }

    /// Tombstones a live entity and returns it. O(log n).
    fn remove(&mut self, idx: Idx) -> Option<T> {
        let id = self.id(idx)?;
        let value = self.slots.get_mut(idx.position())?.take();
        self.by_id.remove(&id);
        value
    }

    /// Live entities in public-ID order. O(1) per item.
    fn iter(&self) -> impl Iterator<Item = (Idx, Id, &T)> {
        self.by_id
            .iter()
            .filter_map(|(&id, &idx)| self.get(idx).map(|t| (idx, id, t)))
    }
}

// ---------------------------------------------------------------------------
// Derived reference counts.
// ---------------------------------------------------------------------------

/// Reference counts derived from the threats. A cache (D10): the threats
/// are the source of truth, and the counts can be rebuilt from them in
/// O(threats x p). They make the "in use" checks O(1) instead of a scan.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RefCounts {
    /// Per asset slot and property (flat, stride = property count, D4): how
    /// many live threats on that asset violate that property.
    asset_properties: Vec<u32>,
    /// Per asset slot: how many live threats target it.
    assets: Vec<u32>,
    /// Per damage scenario slot: how many live threats link to it.
    damages: Vec<u32>,
}

/// Position of (asset, property) in the flat counts.
fn flat(row: usize, a: AssetIdx, p: Property) -> Option<usize> {
    a.index().checked_mul(row)?.checked_add(p.index())
}

/// Adds 1 to (or, with `up == false`, takes 1 from) `v[i]` if it exists.
/// Validation guarantees the slot exists and the count cannot wrap.
fn bump(v: &mut [u32], i: Option<usize>, up: bool) {
    if let Some(c) = i.and_then(|i| v.get_mut(i)) {
        *c = if up {
            c.saturating_add(1)
        } else {
            c.saturating_sub(1)
        };
    }
}

impl RefCounts {
    /// Counts one threat in (`up`) or out. O(p + k).
    fn threat(&mut self, row: usize, t: &ThreatScenario, violates: PropertySet, up: bool) {
        bump(&mut self.assets, Some(t.asset.index()), up);
        self.properties(row, t.asset, violates, up);
        for d in &t.damages {
            bump(&mut self.damages, Some(d.index()), up);
        }
    }

    /// Counts the properties of one threat in or out. O(p).
    fn properties(&mut self, row: usize, a: AssetIdx, violates: PropertySet, up: bool) {
        for p in violates.iter() {
            bump(&mut self.asset_properties, flat(row, a, p), up);
        }
    }
}

// ---------------------------------------------------------------------------
// Project.
// ---------------------------------------------------------------------------

/// A TARA project: assets, damage scenarios, and threat scenarios, checked
/// against one fixed [`Methodology`].
///
/// Every change is an [`Op`]. [`apply`](Self::apply) checks it against the
/// methodology and the referential rules, commits it only if every check
/// passes, and returns the inverse op. On error nothing changes.
///
/// Storage: per entity kind, an arena of slots (a removed entity leaves a
/// tombstone; slots are not reused), a parallel `Vec` of public IDs, and a
/// `BTreeMap` from public ID to index, so lookups are O(log n) and
/// iteration is in public-ID order. Reference counts derived from the
/// threats are kept as a cache.
///
/// Equality is logical: two projects are equal if their methodologies are
/// equal and they hold the same IDs per kind with equal content. Slot
/// layout, tombstones, and the slot limit are ignored. `==` and `clone`
/// both cost O(n + L) (n entities, L damage links and catalog refs); the
/// methodology is shared through its `Arc`, so cloning does not copy it.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, DamageId, Methodology, Op, Project, ThreatId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
///
/// let ecu = AssetId::new_v7();
/// project.apply(Op::AddAsset { id: ecu, name: "Brake ECU".into(), properties: m.all_properties() })?;
///
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let severe = m.level(safety, "severe").ok_or("no level")?;
/// let crash = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id: crash,
///     name: "Vehicle cannot brake".into(),
///     impacts: vec![(safety, severe)],
/// })?;
///
/// let tampering = m.threat_category("tampering").ok_or("no category")?;
/// let threat = ThreatId::new_v7();
/// let undo = project.apply(Op::AddThreatScenario {
///     id: threat,
///     name: "Modify brake firmware".into(),
///     asset: ecu,
///     category: tampering,
///     catalog_refs: vec![],
///     damages: vec![crash],
/// })?;
/// assert_eq!(undo, Op::RemoveThreatScenario { id: threat });
/// assert_eq!(project.threat_scenario_count(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct Project {
    methodology: Arc<Methodology>,
    /// Most slots per arena, tombstones included.
    slot_limit: u32,
    assets: Arena<AssetId, AssetIdx, Asset>,
    damages: Arena<DamageId, DamageIdx, DamageScenario>,
    threats: Arena<ThreatId, ThreatIdx, ThreatScenario>,
    /// Derived data, rebuilt from `threats` (D10).
    refs: RefCounts,
}

/// Checks an entity name. Time O(name length).
fn check_name(name: &str) -> Result<(), ModelError> {
    if name.trim().is_empty() {
        return Err(ModelError::EmptyName);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ModelError::NameTooLong {
            max: MAX_NAME_CHARS,
        });
    }
    Ok(())
}

impl Project {
    /// Creates an empty project over `methodology`, fixed for the
    /// project's lifetime. Each arena may hold up to `u32::MAX` slots.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project};
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.asset_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new(methodology: Arc<Methodology>) -> Self {
        Self::with_slot_limit(methodology, u32::MAX)
    }

    /// Creates an empty project whose arenas each hold at most `limit`
    /// slots (tombstones of removed entities included). An add op beyond
    /// the limit fails with [`ModelError::ProjectCapacityExceeded`]. Use it
    /// to bound the size of a project built from untrusted input.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, ModelError, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::with_slot_limit(Arc::new(Methodology::iso21434_r155()?), 1);
    /// let add = |id| Op::AddDamageScenario { id, name: "d".into(), impacts: vec![] };
    /// project.apply(add(DamageId::new_v7()))?;
    /// assert_eq!(project.apply(add(DamageId::new_v7())), Err(ModelError::ProjectCapacityExceeded));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with_slot_limit(methodology: Arc<Methodology>, limit: u32) -> Self {
        Project {
            methodology,
            slot_limit: limit,
            assets: Arena::new(),
            damages: Arena::new(),
            threats: Arena::new(),
            refs: RefCounts::default(),
        }
    }

    /// Returns the project's methodology. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project};
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(Arc::clone(&m));
    /// assert!(Arc::ptr_eq(project.methodology(), &m));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn methodology(&self) -> &Arc<Methodology> {
        &self.methodology
    }

    // -- Assets ------------------------------------------------------------------

    /// Returns a live asset. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset { id, name: "ECU".into(), properties: m.all_properties() })?;
    /// let idx = project.asset_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.asset(idx).map(|a| a.name()), Some("ECU"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset(&self, idx: AssetIdx) -> Option<&Asset> {
        self.assets.get(idx)
    }

    /// Resolves an asset's public ID. Time O(log n).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.asset_idx(&AssetId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset_idx(&self, id: &AssetId) -> Option<AssetIdx> {
        self.assets.idx(id)
    }

    /// Returns a live asset's public ID. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset { id, name: "ECU".into(), properties: m.all_properties() })?;
    /// let idx = project.asset_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.asset_id(idx), Some(id));
    /// project.apply(Op::RemoveAsset { id })?;
    /// assert_eq!(project.asset_id(idx), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset_id(&self, idx: AssetIdx) -> Option<AssetId> {
        self.assets.id(idx)
    }

    /// Returns the number of live assets. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.asset_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset_count(&self) -> usize {
        self.assets.len()
    }

    /// Iterates live assets in public-ID order. Time O(1) per item, O(n)
    /// for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let (a, b) = (AssetId::new_v7(), AssetId::new_v7());
    /// for id in [b, a] {
    ///     project.apply(Op::AddAsset { id, name: "ECU".into(), properties: m.all_properties() })?;
    /// }
    /// let ids: Vec<_> = project.assets().map(|(_, id, _)| id).collect();
    /// assert_eq!(ids, [a.min(b), a.max(b)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn assets(&self) -> impl Iterator<Item = (AssetIdx, AssetId, &Asset)> {
        self.assets.iter()
    }

    // -- Damage scenarios ----------------------------------------------------------

    /// Returns a live damage scenario. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DamageId, Methodology, Op, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id, name: "Crash".into(), impacts: vec![] })?;
    /// let idx = project.damage_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.damage_scenario(idx).map(|d| d.name()), Some("Crash"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damage_scenario(&self, idx: DamageIdx) -> Option<&DamageScenario> {
        self.damages.get(idx)
    }

    /// Resolves a damage scenario's public ID. Time O(log n).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DamageId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.damage_scenario_idx(&DamageId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damage_scenario_idx(&self, id: &DamageId) -> Option<DamageIdx> {
        self.damages.idx(id)
    }

    /// Returns a live damage scenario's public ID. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DamageId, Methodology, Op, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id, name: "Crash".into(), impacts: vec![] })?;
    /// let idx = project.damage_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.damage_scenario_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damage_scenario_id(&self, idx: DamageIdx) -> Option<DamageId> {
        self.damages.id(idx)
    }

    /// Returns the number of live damage scenarios. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.damage_scenario_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damage_scenario_count(&self) -> usize {
        self.damages.len()
    }

    /// Iterates live damage scenarios in public-ID order. Time O(1) per
    /// item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DamageId, Methodology, Op, Project};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id, name: "Crash".into(), impacts: vec![] })?;
    /// assert_eq!(project.damage_scenarios().map(|(_, id, _)| id).collect::<Vec<_>>(), [id]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn damage_scenarios(&self) -> impl Iterator<Item = (DamageIdx, DamageId, &DamageScenario)> {
        self.damages.iter()
    }

    // -- Threat scenarios ------------------------------------------------------------

    /// Returns a live threat scenario. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ThreatId};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let idx = project.threat_scenario_idx(&ThreatId::new_v7());
    /// assert!(idx.and_then(|i| project.threat_scenario(i)).is_none());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_scenario(&self, idx: ThreatIdx) -> Option<&ThreatScenario> {
        self.threats.get(idx)
    }

    /// Resolves a threat scenario's public ID. Time O(log n).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ThreatId};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.threat_scenario_idx(&ThreatId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_scenario_idx(&self, id: &ThreatId) -> Option<ThreatIdx> {
        self.threats.idx(id)
    }

    /// Returns a live threat scenario's public ID. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// let id = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id, name: "Replay".into(), asset, category, catalog_refs: vec![], damages: vec![],
    /// })?;
    /// let idx = project.threat_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.threat_scenario_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_scenario_id(&self, idx: ThreatIdx) -> Option<ThreatId> {
        self.threats.id(idx)
    }

    /// Returns the number of live threat scenarios. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.threat_scenario_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_scenario_count(&self) -> usize {
        self.threats.len()
    }

    /// Iterates live threat scenarios in public-ID order. Time O(1) per
    /// item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.threat_scenarios().count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn threat_scenarios(&self) -> impl Iterator<Item = (ThreatIdx, ThreatId, &ThreatScenario)> {
        self.threats.iter()
    }

    /// Returns the properties a live threat targets: the ones its category
    /// violates. Derived, so it can never disagree with the category.
    /// Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// let category = m.threat_category("denial-of-service").ok_or("none")?;
    /// let id = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id, name: "Flood CAN bus".into(), asset, category, catalog_refs: vec![], damages: vec![],
    /// })?;
    /// let idx = project.threat_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.targets(idx), Some(m.violates(category)));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn targets(&self, idx: ThreatIdx) -> Option<PropertySet> {
        self.threats
            .get(idx)
            .map(|t| self.methodology.violates(t.category))
    }

    // -- Ops ---------------------------------------------------------------------------

    /// Checks `op` against the methodology and the referential rules
    /// without changing anything. Returns exactly the error
    /// [`apply`](Self::apply) would return.
    ///
    /// Time: the cost of the op (see [`Op`]); never O(project size).
    ///
    /// # Errors
    ///
    /// The first failed check, in this order: the target or new ID; the
    /// name; vocabulary indices against the methodology; the entity's own
    /// rules (non-empty properties, no repeated category, the asset carries
    /// the threat category's properties, unique and at most 16 catalog
    /// refs, existing and unique damage links); referential rules (no
    /// removal or property drop while referenced, links and refs must or
    /// must not exist); and capacity. See [`ModelError`] for each variant.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, ModelError, Op, Project, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = AssetId::new_v7();
    /// let op = Op::AddAsset { id, name: "ECU".into(), properties: PropertySet::empty() };
    /// assert_eq!(project.validate(&op), Err(ModelError::NoSecurityProperties(id)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn validate(&self, op: &Op) -> Result<(), ModelError> {
        match op {
            Op::AddAsset {
                id,
                name,
                properties,
            } => self.check_add_asset(*id, name, *properties),
            Op::RenameAsset { id, name } => {
                self.live_asset(*id)?;
                check_name(name)
            }
            Op::SetAssetProperties { id, properties } => {
                self.check_set_asset_properties(*id, *properties)
            }
            Op::RemoveAsset { id } => self.check_remove_asset(*id),
            Op::AddDamageScenario { id, name, impacts } => {
                self.check_add_damage(*id, name, impacts)
            }
            Op::RenameDamageScenario { id, name } => {
                self.live_damage(*id)?;
                check_name(name)
            }
            Op::SetImpact {
                id,
                category,
                level,
            } => {
                self.live_damage(*id)?;
                self.check_rating(*category, *level)
            }
            Op::RemoveDamageScenario { id } => self.check_remove_damage(*id),
            Op::AddThreatScenario {
                id,
                name,
                asset,
                category,
                catalog_refs,
                damages,
            } => self.check_add_threat(*id, name, *asset, *category, catalog_refs, damages),
            Op::RenameThreatScenario { id, name } => {
                self.live_threat(*id)?;
                check_name(name)
            }
            Op::SetThreatCategory { id, category } => {
                let t = self.threat_ref(*id)?;
                let (asset_id, asset) = self.threat_asset(*id, t)?;
                self.check_category_fits(asset_id, asset, *category)
            }
            Op::AddCatalogRef {
                threat,
                catalog_ref,
            } => self.check_catalog_ref_change(*threat, catalog_ref, true),
            Op::RemoveCatalogRef {
                threat,
                catalog_ref,
            } => self.check_catalog_ref_change(*threat, catalog_ref, false),
            Op::LinkDamage { threat, damage } => self.check_link(*threat, *damage, true),
            Op::UnlinkDamage { threat, damage } => self.check_link(*threat, *damage, false),
            Op::RemoveThreatScenario { id } => self.live_threat(*id).map(|_| ()),
        }
    }

    /// Checks `op` like [`validate`](Self::validate), then commits it and
    /// returns its inverse. Applying the inverse right away restores a
    /// project equal to the one before.
    ///
    /// Time: the cost of the op (see [`Op`]); never O(project size). Space:
    /// O(size of the op) for the inverse.
    ///
    /// # Errors
    ///
    /// The same error [`validate`](Self::validate) returns. On error the
    /// project is unchanged.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id, name: "Fraud".into(), impacts: vec![] })?;
    /// let before = project.clone();
    /// let cat = m.impact_category("financial").ok_or("none")?;
    /// let level = m.level(cat, "major");
    /// let undo = project.apply(Op::SetImpact { id, category: cat, level })?;
    /// assert_eq!(undo, Op::SetImpact { id, category: cat, level: None });
    /// project.apply(undo)?;
    /// assert_eq!(project, before);
    /// # Ok(())
    /// # }
    /// ```
    pub fn apply(&mut self, op: Op) -> Result<Op, ModelError> {
        self.validate(&op)?;
        self.commit(op)
    }

    /// Commits a validated op. Every fallible lookup happens before the
    /// first mutation, so even an unexpected error leaves the project
    /// unchanged.
    fn commit(&mut self, op: Op) -> Result<Op, ModelError> {
        match op {
            Op::AddAsset {
                id,
                name,
                properties,
            } => self.commit_add_asset(id, name, properties),
            Op::RenameAsset { id, name } => {
                let name = std::mem::replace(&mut self.asset_mut(id)?.name, name);
                Ok(Op::RenameAsset { id, name })
            }
            Op::SetAssetProperties { id, properties } => {
                let properties = std::mem::replace(&mut self.asset_mut(id)?.properties, properties);
                Ok(Op::SetAssetProperties { id, properties })
            }
            Op::RemoveAsset { id } => {
                let idx = self.asset_index(id)?;
                let a = self
                    .assets
                    .remove(idx)
                    .ok_or(ModelError::UnknownAsset(id))?;
                Ok(Op::AddAsset {
                    id,
                    name: a.name,
                    properties: a.properties,
                })
            }
            Op::AddDamageScenario { id, name, impacts } => {
                self.commit_add_damage(id, name, &impacts)
            }
            Op::RenameDamageScenario { id, name } => {
                let name = std::mem::replace(&mut self.damage_mut(id)?.name, name);
                Ok(Op::RenameDamageScenario { id, name })
            }
            Op::SetImpact {
                id,
                category,
                level,
            } => self.commit_set_impact(id, category, level),
            Op::RemoveDamageScenario { id } => self.commit_remove_damage(id),
            Op::AddThreatScenario {
                id,
                name,
                asset,
                category,
                catalog_refs,
                damages,
            } => self.commit_add_threat(id, name, asset, category, catalog_refs, damages),
            Op::RenameThreatScenario { id, name } => {
                let name = std::mem::replace(&mut self.threat_mut(id)?.name, name);
                Ok(Op::RenameThreatScenario { id, name })
            }
            Op::SetThreatCategory { id, category } => self.commit_set_category(id, category),
            Op::AddCatalogRef {
                threat,
                catalog_ref,
            } => self.commit_catalog_ref(threat, catalog_ref, true),
            Op::RemoveCatalogRef {
                threat,
                catalog_ref,
            } => self.commit_catalog_ref(threat, catalog_ref, false),
            Op::LinkDamage { threat, damage } => self.commit_link(threat, damage, true),
            Op::UnlinkDamage { threat, damage } => self.commit_link(threat, damage, false),
            Op::RemoveThreatScenario { id } => self.commit_remove_threat(id),
        }
    }

    fn asset_mut(&mut self, id: AssetId) -> Result<&mut Asset, ModelError> {
        let idx = self.asset_index(id)?;
        self.assets.get_mut(idx).ok_or(ModelError::UnknownAsset(id))
    }

    fn damage_mut(&mut self, id: DamageId) -> Result<&mut DamageScenario, ModelError> {
        let idx = self.damage_index(id)?;
        self.damages
            .get_mut(idx)
            .ok_or(ModelError::UnknownDamageScenario(id))
    }

    fn threat_mut(&mut self, id: ThreatId) -> Result<&mut ThreatScenario, ModelError> {
        let idx = self.threat_index(id)?;
        self.threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(id))
    }

    // -- Lookups shared by checks and commits ----------------------------------

    fn asset_index(&self, id: AssetId) -> Result<AssetIdx, ModelError> {
        self.assets.idx(&id).ok_or(ModelError::UnknownAsset(id))
    }

    fn damage_index(&self, id: DamageId) -> Result<DamageIdx, ModelError> {
        self.damages
            .idx(&id)
            .ok_or(ModelError::UnknownDamageScenario(id))
    }

    fn threat_index(&self, id: ThreatId) -> Result<ThreatIdx, ModelError> {
        self.threats
            .idx(&id)
            .ok_or(ModelError::UnknownThreatScenario(id))
    }

    fn live_asset(&self, id: AssetId) -> Result<(AssetIdx, &Asset), ModelError> {
        let idx = self.asset_index(id)?;
        let a = self.assets.get(idx).ok_or(ModelError::UnknownAsset(id))?;
        Ok((idx, a))
    }

    fn live_damage(&self, id: DamageId) -> Result<DamageIdx, ModelError> {
        let idx = self.damage_index(id)?;
        self.damages
            .get(idx)
            .map(|_| idx)
            .ok_or(ModelError::UnknownDamageScenario(id))
    }

    fn live_threat(&self, id: ThreatId) -> Result<ThreatIdx, ModelError> {
        let idx = self.threat_index(id)?;
        self.threats
            .get(idx)
            .map(|_| idx)
            .ok_or(ModelError::UnknownThreatScenario(id))
    }

    /// The live asset a live threat targets. A threat's asset is always
    /// live (removal is rejected while referenced).
    fn threat_asset(
        &self,
        id: ThreatId,
        t: &ThreatScenario,
    ) -> Result<(AssetId, &Asset), ModelError> {
        let asset = self.assets.get(t.asset);
        let asset_id = self.assets.id(t.asset);
        asset_id
            .zip(asset)
            .ok_or(ModelError::UnknownThreatScenario(id))
    }

    fn threat_ref(&self, id: ThreatId) -> Result<&ThreatScenario, ModelError> {
        let idx = self.threat_index(id)?;
        self.threats
            .get(idx)
            .ok_or(ModelError::UnknownThreatScenario(id))
    }

    /// The public ID of a linked damage scenario. Linked damage scenarios
    /// are always live.
    fn damage_key(&self, idx: DamageIdx) -> Option<DamageId> {
        self.damages.id(idx)
    }

    /// Length of one asset's row in the flat per-property counts: the
    /// property count of the methodology.
    fn row_len(&self) -> usize {
        self.methodology.property_count()
    }

    fn asset_property_refs(&self, a: AssetIdx, p: Property) -> u32 {
        flat(self.row_len(), a, p)
            .and_then(|i| self.refs.asset_properties.get(i))
            .copied()
            .unwrap_or(0)
    }

    // -- Checks ----------------------------------------------------------------------

    fn check_add_asset(
        &self,
        id: AssetId,
        name: &str,
        properties: PropertySet,
    ) -> Result<(), ModelError> {
        if self.assets.idx(&id).is_some() {
            return Err(ModelError::DuplicateAsset(id));
        }
        check_name(name)?;
        self.methodology.check_properties(properties)?;
        if properties.is_empty() {
            return Err(ModelError::NoSecurityProperties(id));
        }
        // The flat per-property counts must stay addressable.
        let flat_ok = self
            .assets
            .slot_count()
            .checked_add(1)
            .and_then(|n| n.checked_mul(self.row_len()))
            .is_some();
        if self.assets.next_idx(self.slot_limit).is_none() || !flat_ok {
            return Err(ModelError::ProjectCapacityExceeded);
        }
        Ok(())
    }

    fn check_set_asset_properties(
        &self,
        id: AssetId,
        properties: PropertySet,
    ) -> Result<(), ModelError> {
        let (idx, asset) = self.live_asset(id)?;
        self.methodology.check_properties(properties)?;
        if properties.is_empty() {
            return Err(ModelError::NoSecurityProperties(id));
        }
        for p in asset.properties.iter().filter(|&p| !properties.contains(p)) {
            let threats = self.asset_property_refs(idx, p);
            if threats > 0 {
                return Err(ModelError::PropertyInUse {
                    asset: id,
                    property: p,
                    threats,
                });
            }
        }
        Ok(())
    }

    fn check_remove_asset(&self, id: AssetId) -> Result<(), ModelError> {
        let (idx, _) = self.live_asset(id)?;
        let threats = self.refs.assets.get(idx.index()).copied().unwrap_or(0);
        if threats > 0 {
            return Err(ModelError::AssetInUse { asset: id, threats });
        }
        Ok(())
    }

    fn check_rating(
        &self,
        category: ImpactCategory,
        level: Option<Level>,
    ) -> Result<(), ModelError> {
        match level {
            Some(level) => self.methodology.check_level(category, level),
            None => self.methodology.check_impact_category(category),
        }
    }

    fn check_add_damage(
        &self,
        id: DamageId,
        name: &str,
        impacts: &[(ImpactCategory, Level)],
    ) -> Result<(), ModelError> {
        if self.damages.idx(&id).is_some() {
            return Err(ModelError::DuplicateDamageScenario(id));
        }
        check_name(name)?;
        // At most 32 categories, so one bit each fits a `u32`.
        let mut seen = 0u32;
        for &(category, level) in impacts {
            self.methodology.check_level(category, level)?;
            let bit = u32::try_from(category.index())
                .ok()
                .and_then(|i| 1u32.checked_shl(i))
                .unwrap_or(0);
            if seen & bit != 0 {
                return Err(ModelError::DuplicateImpactCategory(category));
            }
            seen |= bit;
        }
        if self.damages.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded);
        }
        Ok(())
    }

    fn check_remove_damage(&self, id: DamageId) -> Result<(), ModelError> {
        let idx = self.live_damage(id)?;
        let threats = self.refs.damages.get(idx.index()).copied().unwrap_or(0);
        if threats > 0 {
            return Err(ModelError::DamageScenarioInUse {
                damage: id,
                threats,
            });
        }
        Ok(())
    }

    /// Checks that `category` exists and the asset carries every property
    /// it violates. O(p).
    fn check_category_fits(
        &self,
        asset_id: AssetId,
        a: &Asset,
        category: ThreatCategory,
    ) -> Result<(), ModelError> {
        self.methodology.check_threat_category(category)?;
        match self
            .methodology
            .violates(category)
            .iter()
            .find(|&p| !a.properties.contains(p))
        {
            Some(property) => Err(ModelError::PropertyNotOnAsset {
                asset: asset_id,
                property,
            }),
            None => Ok(()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_add_threat(
        &self,
        id: ThreatId,
        name: &str,
        asset: AssetId,
        category: ThreatCategory,
        catalog_refs: &[CatalogRef],
        damages: &[DamageId],
    ) -> Result<(), ModelError> {
        if self.threats.idx(&id).is_some() {
            return Err(ModelError::DuplicateThreatScenario(id));
        }
        check_name(name)?;
        let (_, a) = self.live_asset(asset)?;
        self.check_category_fits(asset, a, category)?;
        if catalog_refs.len() > MAX_CATALOG_REFS {
            return Err(ModelError::TooManyCatalogRefs {
                threat: id,
                max: MAX_CATALOG_REFS,
            });
        }
        for r in catalog_refs {
            self.methodology.check_catalog_ref(r)?;
        }
        let mut refs = catalog_refs.to_vec();
        refs.sort_unstable();
        if refs.windows(2).any(|w| w.first() == w.get(1)) {
            return Err(ModelError::DuplicateCatalogRef { threat: id });
        }
        for &d in damages {
            self.live_damage(d)?;
        }
        let mut ids = damages.to_vec();
        ids.sort_unstable();
        if let Some(w) = ids.windows(2).find(|w| w.first() == w.get(1)) {
            if let Some(&damage) = w.first() {
                return Err(ModelError::DuplicateDamageLink { threat: id, damage });
            }
        }
        if self.threats.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded);
        }
        Ok(())
    }

    fn check_catalog_ref_change(
        &self,
        threat: ThreatId,
        r: &CatalogRef,
        add: bool,
    ) -> Result<(), ModelError> {
        let t = self.threat_ref(threat)?;
        self.methodology.check_catalog_ref(r)?;
        let present = t.catalog_refs.binary_search(r).is_ok();
        match (add, present) {
            (true, true) => Err(ModelError::DuplicateCatalogRef { threat }),
            (true, false) if t.catalog_refs.len() >= MAX_CATALOG_REFS => {
                Err(ModelError::TooManyCatalogRefs {
                    threat,
                    max: MAX_CATALOG_REFS,
                })
            }
            (false, false) => Err(ModelError::CatalogRefNotFound { threat }),
            _ => Ok(()),
        }
    }

    /// Finds `damage` in a threat's sorted links: `Ok(pos)` if linked,
    /// `Err(pos)` for the insertion point. O(log k).
    fn find_link(&self, t: &ThreatScenario, damage: DamageId) -> Result<usize, usize> {
        t.damages
            .binary_search_by(|&d| self.damage_key(d).cmp(&Some(damage)))
    }

    fn check_link(&self, threat: ThreatId, damage: DamageId, link: bool) -> Result<(), ModelError> {
        let t = self.threat_ref(threat)?;
        self.live_damage(damage)?;
        match (link, self.find_link(t, damage).is_ok()) {
            (true, true) => Err(ModelError::DuplicateDamageLink { threat, damage }),
            (false, false) => Err(ModelError::DamageNotLinked { threat, damage }),
            _ => Ok(()),
        }
    }

    // -- Commits -----------------------------------------------------------------------

    fn commit_add_asset(
        &mut self,
        id: AssetId,
        name: Arc<str>,
        properties: PropertySet,
    ) -> Result<Op, ModelError> {
        let asset = Asset { name, properties };
        self.assets
            .push(id, asset, self.slot_limit)
            .ok_or(ModelError::ProjectCapacityExceeded)?;
        let row = self.row_len();
        let len = self.refs.asset_properties.len().saturating_add(row);
        self.refs.asset_properties.resize(len, 0);
        self.refs.assets.push(0);
        Ok(Op::RemoveAsset { id })
    }

    fn commit_add_damage(
        &mut self,
        id: DamageId,
        name: Arc<str>,
        impacts: &[(ImpactCategory, Level)],
    ) -> Result<Op, ModelError> {
        let mut slots = vec![None; self.methodology.impact_category_count()];
        for &(category, level) in impacts {
            let slot = slots
                .get_mut(category.index())
                .ok_or(ModelError::UnknownImpactCategory(category))?;
            *slot = Some(level);
        }
        let d = DamageScenario {
            name,
            impacts: slots.into_boxed_slice(),
        };
        self.damages
            .push(id, d, self.slot_limit)
            .ok_or(ModelError::ProjectCapacityExceeded)?;
        self.refs.damages.push(0);
        Ok(Op::RemoveDamageScenario { id })
    }

    fn commit_set_impact(
        &mut self,
        id: DamageId,
        category: ImpactCategory,
        level: Option<Level>,
    ) -> Result<Op, ModelError> {
        let slot = self
            .damage_mut(id)?
            .impacts
            .get_mut(category.index())
            .ok_or(ModelError::UnknownImpactCategory(category))?;
        let level = std::mem::replace(slot, level);
        Ok(Op::SetImpact {
            id,
            category,
            level,
        })
    }

    fn commit_remove_damage(&mut self, id: DamageId) -> Result<Op, ModelError> {
        let idx = self.damage_index(id)?;
        let d = self
            .damages
            .remove(idx)
            .ok_or(ModelError::UnknownDamageScenario(id))?;
        let impacts = d.impacts().filter_map(|(c, l)| l.map(|l| (c, l))).collect();
        Ok(Op::AddDamageScenario {
            id,
            name: d.name,
            impacts,
        })
    }

    fn commit_catalog_ref(
        &mut self,
        threat: ThreatId,
        catalog_ref: CatalogRef,
        add: bool,
    ) -> Result<Op, ModelError> {
        let refs = &mut self.threat_mut(threat)?.catalog_refs;
        match (add, refs.binary_search(&catalog_ref)) {
            (true, Err(pos)) => {
                refs.insert(pos, catalog_ref);
                Ok(Op::RemoveCatalogRef {
                    threat,
                    catalog_ref,
                })
            }
            (false, Ok(pos)) => {
                refs.remove(pos);
                Ok(Op::AddCatalogRef {
                    threat,
                    catalog_ref,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateCatalogRef { threat }),
            (false, Err(_)) => Err(ModelError::CatalogRefNotFound { threat }),
        }
    }

    fn commit_add_threat(
        &mut self,
        id: ThreatId,
        name: Arc<str>,
        asset: AssetId,
        category: ThreatCategory,
        mut catalog_refs: Vec<CatalogRef>,
        mut damages: Vec<DamageId>,
    ) -> Result<Op, ModelError> {
        let asset = self.asset_index(asset)?;
        damages.sort_unstable();
        let damages = damages
            .into_iter()
            .map(|d| self.damage_index(d))
            .collect::<Result<Vec<_>, _>>()?;
        catalog_refs.sort_unstable();
        let t = ThreatScenario {
            name,
            asset,
            category,
            catalog_refs,
            damages,
        };
        let row = self.row_len();
        let violates = self.methodology.violates(category);
        let idx = self
            .threats
            .push(id, t, self.slot_limit)
            .ok_or(ModelError::ProjectCapacityExceeded)?;
        if let Some(t) = self.threats.get(idx) {
            self.refs.threat(row, t, violates, true);
        }
        Ok(Op::RemoveThreatScenario { id })
    }

    fn commit_set_category(
        &mut self,
        id: ThreatId,
        category: ThreatCategory,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(id)?;
        let row = self.row_len();
        let new = self.methodology.violates(category);
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        let old_category = std::mem::replace(&mut t.category, category);
        let asset = t.asset;
        let old = self.methodology.violates(old_category);
        self.refs.properties(row, asset, old, false);
        self.refs.properties(row, asset, new, true);
        Ok(Op::SetThreatCategory {
            id,
            category: old_category,
        })
    }

    fn commit_link(
        &mut self,
        threat: ThreatId,
        damage: DamageId,
        link: bool,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(threat)?;
        let d = self.damage_index(damage)?;
        let t = self
            .threats
            .get(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?;
        let found = self.find_link(t, damage);
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?;
        match (link, found) {
            (true, Err(pos)) => {
                t.damages.insert(pos, d);
                bump(&mut self.refs.damages, Some(d.index()), true);
                Ok(Op::UnlinkDamage { threat, damage })
            }
            (false, Ok(pos)) => {
                t.damages.remove(pos);
                bump(&mut self.refs.damages, Some(d.index()), false);
                Ok(Op::LinkDamage { threat, damage })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateDamageLink { threat, damage }),
            (false, Err(_)) => Err(ModelError::DamageNotLinked { threat, damage }),
        }
    }

    fn commit_remove_threat(&mut self, id: ThreatId) -> Result<Op, ModelError> {
        let idx = self.threat_index(id)?;
        let t = self.threat_ref(id)?;
        let asset = self
            .assets
            .id(t.asset)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        let damages = t
            .damages
            .iter()
            .map(|&d| {
                self.damage_key(d)
                    .ok_or(ModelError::UnknownThreatScenario(id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let t = self
            .threats
            .remove(idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        let violates = self.methodology.violates(t.category);
        self.refs.threat(self.row_len(), &t, violates, false);
        Ok(Op::AddThreatScenario {
            id,
            name: t.name,
            asset,
            category: t.category,
            catalog_refs: t.catalog_refs,
            damages,
        })
    }
}

impl PartialEq for Project {
    /// Logical equality: equal methodologies (pointer or content), the same
    /// IDs per kind, and equal content, with threat references compared by
    /// public ID. Slot layout, tombstones, and the slot limit are ignored.
    ///
    /// Time O(n + L + m): n entities, L damage links and catalog refs, m
    /// methodology size.
    fn eq(&self, other: &Self) -> bool {
        let same_methodology = Arc::ptr_eq(&self.methodology, &other.methodology)
            || *self.methodology == *other.methodology;
        same_methodology
            && self.assets.len() == other.assets.len()
            && self.damages.len() == other.damages.len()
            && self.threats.len() == other.threats.len()
            && self
                .assets()
                .zip(other.assets())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .damage_scenarios()
                .zip(other.damage_scenarios())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .threat_scenarios()
                .zip(other.threat_scenarios())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_threat(x, other, y))
    }
}

impl Eq for Project {}

impl Project {
    /// Compares two threats from two projects by content, with references
    /// resolved to public IDs. O(k).
    fn same_threat(&self, x: &ThreatScenario, other: &Project, y: &ThreatScenario) -> bool {
        x.name == y.name
            && x.category == y.category
            && x.catalog_refs == y.catalog_refs
            && self.assets.id(x.asset) == other.assets.id(y.asset)
            && x.damages.len() == y.damages.len()
            && x.damages
                .iter()
                .zip(&y.damages)
                .all(|(&a, &b)| self.damage_key(a) == other.damage_key(b))
    }

    /// Checks every structural invariant and returns a description of the
    /// first one broken. Time O(n + L + m).
    #[cfg(test)]
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        let m = &*self.methodology;
        arena_invariants("asset", &self.assets)?;
        arena_invariants("damage", &self.damages)?;
        arena_invariants("threat", &self.threats)?;

        for (_, id, a) in self.assets() {
            check_name(&a.name).map_err(|e| format!("asset {id}: {e}"))?;
            if a.properties.is_empty() {
                return Err(format!("asset {id} has no properties"));
            }
            m.check_properties(a.properties)
                .map_err(|e| format!("asset {id}: {e}"))?;
        }
        for (_, id, d) in self.damage_scenarios() {
            check_name(&d.name).map_err(|e| format!("damage {id}: {e}"))?;
            if d.impacts.len() != m.impact_category_count() {
                return Err(format!("damage {id} has {} impact slots", d.impacts.len()));
            }
            for (c, l) in d.impacts() {
                if let Some(l) = l {
                    m.check_level(c, l)
                        .map_err(|e| format!("damage {id}: {e}"))?;
                }
            }
        }

        let row = self.row_len();
        let mut want = RefCounts {
            asset_properties: vec![0; self.assets.slot_count() * row],
            assets: vec![0; self.assets.slot_count()],
            damages: vec![0; self.damages.slot_count()],
        };
        for (_, id, t) in self.threat_scenarios() {
            check_name(&t.name).map_err(|e| format!("threat {id}: {e}"))?;
            m.check_threat_category(t.category)
                .map_err(|e| format!("threat {id}: {e}"))?;
            let asset = self
                .assets
                .get(t.asset)
                .ok_or(format!("threat {id} targets a dead asset slot"))?;
            if !m.violates(t.category).is_subset(asset.properties) {
                return Err(format!("threat {id}: asset lacks a targeted property"));
            }
            if t.catalog_refs.len() > MAX_CATALOG_REFS {
                return Err(format!("threat {id} has too many catalog refs"));
            }
            if !t.catalog_refs.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!("threat {id} catalog refs not sorted and unique"));
            }
            for r in &t.catalog_refs {
                m.check_catalog_ref(r)
                    .map_err(|e| format!("threat {id}: {e}"))?;
            }
            let keys: Vec<_> = t.damages.iter().map(|&d| self.damage_key(d)).collect();
            if keys.iter().any(Option::is_none) {
                return Err(format!("threat {id} links a dead damage slot"));
            }
            if !keys.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!("threat {id} damages not sorted and unique"));
            }
            want.threat(row, t, m.violates(t.category), true);
        }
        if want != self.refs {
            return Err(format!(
                "reference counts differ from a recount: {:?} vs {:?}",
                self.refs, want
            ));
        }
        Ok(())
    }
}

/// Checks that an arena's slots, IDs, and map agree. O(n log n).
#[cfg(test)]
fn arena_invariants<Id: Copy + Ord + std::fmt::Display, Idx: ArenaIndex, T>(
    kind: &str,
    arena: &Arena<Id, Idx, T>,
) -> Result<(), String> {
    if arena.slots.len() != arena.ids.len() {
        return Err(format!("{kind}: slots and ids differ in length"));
    }
    let live = arena.slots.iter().filter(|s| s.is_some()).count();
    if live != arena.by_id.len() {
        return Err(format!(
            "{kind}: {live} live slots but {} map entries",
            arena.by_id.len()
        ));
    }
    for (id, &idx) in &arena.by_id {
        if arena.get(idx).is_none() {
            return Err(format!("{kind} {id} maps to a dead slot"));
        }
        if arena.ids.get(idx.position()) != Some(id) {
            return Err(format!("{kind} {id} maps to a slot with another ID"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;
    use proptest::prelude::*;
    use proptest::test_runner::TestCaseError;

    fn invariants(p: &Project) -> Result<(), TestCaseError> {
        p.check_invariants().map_err(TestCaseError::fail)
    }

    #[test]
    fn check_name_rules() {
        assert_eq!(check_name("a"), Ok(()));
        assert_eq!(check_name(" a "), Ok(()));
        assert_eq!(check_name(""), Err(ModelError::EmptyName));
        assert_eq!(check_name(" \t\n"), Err(ModelError::EmptyName));
        assert_eq!(check_name(&"é".repeat(256)), Ok(()));
        assert_eq!(
            check_name(&"é".repeat(257)),
            Err(ModelError::NameTooLong { max: 256 })
        );
    }

    #[test]
    fn arena_tombstones_are_not_reused() {
        let mut a: Arena<u32, AssetIdx, &str> = Arena::new();
        let x = a.push(10, "x", u32::MAX).expect("room");
        let y = a.push(5, "y", u32::MAX).expect("room");
        assert_eq!((x.index(), y.index()), (0, 1));
        assert_eq!(a.iter().map(|(_, id, _)| id).collect::<Vec<_>>(), [5, 10]);
        assert_eq!(a.remove(x), Some("x"));
        assert_eq!(a.remove(x), None);
        assert_eq!((a.len(), a.slot_count()), (1, 2));
        assert_eq!(a.get(x), None);
        assert_eq!(a.id(x), None);
        assert_eq!(a.idx(&10), None);
        let z = a.push(10, "z", u32::MAX).expect("room");
        assert_eq!(z.index(), 2);
        assert_eq!(a.id(z), Some(10));
        arena_invariants("test", &a).expect("consistent");
    }

    #[test]
    fn arena_respects_slot_limit() {
        let mut a: Arena<u32, DamageIdx, ()> = Arena::new();
        assert!(a.push(1, (), 1).is_some());
        assert!(a.next_idx(1).is_none());
        assert!(a.push(2, (), 1).is_none());
        assert_eq!((a.len(), a.slot_count()), (1, 1));
    }

    #[test]
    fn bump_saturates_and_ignores_missing_slots() {
        let mut v = vec![0u32, u32::MAX];
        bump(&mut v, Some(0), false);
        bump(&mut v, Some(1), true);
        bump(&mut v, Some(7), true);
        bump(&mut v, None, true);
        assert_eq!(v, [0, u32::MAX]);
    }

    #[test]
    fn check_invariants_detects_a_stale_count() {
        let m = testkit::builtin();
        let mut p = Project::new(Arc::clone(&m));
        let a = testkit::asset_id(1);
        p.apply(Op::AddAsset {
            id: a,
            name: "a".into(),
            properties: m.all_properties(),
        })
        .expect("valid");
        assert_eq!(p.check_invariants(), Ok(()));
        p.refs.assets[0] = 3;
        assert!(p.check_invariants().is_err());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn project_invariants_first_methodology(seeds in testkit::seeds()) {
            testkit::replay(&testkit::builtin(), &seeds, invariants)?;
        }

        #[test]
        fn project_invariants_second_methodology(seeds in testkit::seeds()) {
            testkit::replay(&testkit::second(), &seeds, invariants)?;
        }
    }
}
