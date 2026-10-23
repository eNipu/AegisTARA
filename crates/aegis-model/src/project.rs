//! The project: assets, damage scenarios, threat scenarios, and attack
//! trees over one methodology, changed only through typed [`Op`]s.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::asset::{Asset, PropertySet};
use crate::control::{MAX_STEP_MITIGATIONS, RiskTreatment, SecurityControl, StepMitigation};
use crate::damage::DamageScenario;
use crate::diff::{EntityDiff, ProjectDiff, RiskDelta, RiskSnapshot, ThreatRiskDelta};
use crate::error::{EntityKind, ModelError};
use crate::eval::{Evaluation, MaxMin, evaluate};
use crate::id::{
    ArenaIndex, AssetId, AssetIdx, ControlId, ControlIdx, DamageId, DamageIdx, NodeId, NodeIdx,
    ThreatId, ThreatIdx, TreeId, TreeIdx,
};
use crate::lint::{LintCode, LintFinding};
use crate::methodology::{
    CatalogRef, FeasibilityLevel, ImpactCategory, Level, Methodology, Property, ThreatCategory,
};
use crate::op::Op;
use crate::paths::AttackPath;
use crate::provenance::Provenance;
use crate::threat::{ImpactRisk, MAX_CATALOG_REFS, MAX_DAMAGE_LINKS, ThreatRisk, ThreatScenario};
use crate::tree::{AttackTree, MAX_ATTACK_TREE_NODES, Node};

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
    #[cfg(test)]
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

/// One attack tree in a [`Project`], paired with its per-node step
/// feasibility ratings, per-node step mitigations, and provenance. Gate
/// slots are always `None` and empty.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TreeEntry {
    tree: AttackTree,
    step_feasibility: Vec<Option<FeasibilityLevel>>,
    step_mitigations: Vec<Box<[StepMitigation]>>,
    provenance: Provenance,
}

impl TreeEntry {
    fn residual_step_feasibility(&self, idx: NodeIdx) -> Option<FeasibilityLevel> {
        let inh = self.step_feasibility.get(idx.index()).copied().flatten()?;
        let mitigations = self
            .step_mitigations
            .get(idx.index())
            .map_or(&[][..], Box::as_ref);
        Some(
            mitigations
                .iter()
                .fold(inh, |acc, m| acc.min(m.feasibility)),
        )
    }
}

/// One counted property on one asset: how many live threats on that asset
/// violate it. Never stored with `threats == 0`. 8 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyCount {
    property: Property,
    threats: u32,
}

/// Reference counts derived from the threats. A cache (D10): the threats
/// are the source of truth, and the counts can be rebuilt from them in
/// O(threats x p). They make the "in use" checks O(1) or O(log p) instead
/// of a scan.
///
/// Layout (ADR 0010): the per-property counts are sparse. Each asset slot
/// holds one exact-size boxed slice of the properties that at least one
/// live threat on that asset violates, sorted by property, with no
/// duplicates and no zero counts. A slot no threat targets (tombstones
/// included) holds an empty slice, which costs 16 bytes and no heap. This
/// canonical form makes the derived `==` an exact comparison.
///
/// Space: O(asset slots + damage slots + tree slots + targeted (asset,
/// property) pairs): 16 + 4 bytes per asset slot, 4 bytes per damage slot,
/// 4 bytes per tree slot, 8 bytes per targeted pair (before: 4 bytes x
/// methodology properties per asset slot). Time: a lookup is a binary
/// search, O(log p); counting one threat in or out is O(r + p), with r the
/// row length and p = |violates|, both at most 64; it allocates once only
/// when the row's set of properties changes. `clone` is O(slots + pairs)
/// with one allocation per targeted asset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RefCounts {
    /// Per asset slot: the properties some live threat on that asset
    /// violates, with how many threats violate each (see the type doc).
    asset_properties: Vec<Box<[PropertyCount]>>,
    /// Per asset slot: how many live threats target it.
    assets: Vec<u32>,
    /// Per damage scenario slot: how many live threats link to it.
    damages: Vec<u32>,
    /// Per attack tree slot: how many live threats reference it.
    trees: Vec<u32>,
    /// Per security control slot: how many step mitigations across live
    /// attack trees reference it.
    controls: Vec<u32>,
}

/// Adds 1 to (or, with `up == false`, takes 1 from) `v[i]` if it exists.
/// Validation guarantees the slot exists and the count cannot wrap.
fn bump(v: &mut [u32], i: Option<usize>, up: bool) {
    if let Some(c) = i.and_then(|i| v.get_mut(i)) {
        *c = step(*c, up);
    }
}

/// `c + 1` (`up`) or `c - 1`, saturating.
fn step(c: u32, up: bool) -> u32 {
    if up {
        c.saturating_add(1)
    } else {
        c.saturating_sub(1)
    }
}

/// The count of `p` in one sorted row; 0 if absent. O(log r).
fn row_count(row: &[PropertyCount], p: Property) -> u32 {
    row.binary_search_by_key(&p, |c| c.property)
        .ok()
        .and_then(|i| row.get(i))
        .map_or(0, |c| c.threats)
}

/// Counts the properties in `violates` in (`up`) or out of one sorted
/// row, keeping it canonical (sorted, no zero counts, exact size).
///
/// When the row's set of properties does not change (counting in only
/// properties it has, or counting out none that drop to 0), the counts
/// change in place with no allocation. Otherwise, the row and `violates`
/// merge into one new exact-size slice: one allocation, or none if the
/// row becomes empty. Counting out a property the row lacks is ignored
/// (validation rules it out). Time O(r + p), r = row length, p =
/// |violates|.
fn count_row(row: &mut Box<[PropertyCount]>, violates: PropertySet, up: bool) {
    let have: PropertySet = row.iter().map(|c| c.property).collect();
    let same_shape = if up {
        violates.is_subset(have)
    } else {
        row.iter()
            .all(|c| c.threats > 1 || !violates.contains(c.property))
    };
    if same_shape {
        for c in row.iter_mut().filter(|c| violates.contains(c.property)) {
            c.threats = step(c.threats, up);
        }
        return;
    }
    // Candidates in ascending order: the row's properties, plus the new
    // ones when counting in.
    let candidates = if up {
        have.iter().chain(violates.iter()).collect()
    } else {
        have
    };
    let mut old = row.iter().copied().peekable();
    let mut merged = Vec::with_capacity(new_row_len(row, violates, up, candidates));
    for property in candidates.iter() {
        let before = old
            .next_if(|c| c.property == property)
            .map_or(0, |c| c.threats);
        let threats = if violates.contains(property) {
            step(before, up)
        } else {
            before
        };
        if threats > 0 {
            merged.push(PropertyCount { property, threats });
        }
    }
    *row = merged.into_boxed_slice();
}

/// The length of a row after [`count_row`] changes its shape, so the new
/// slice is allocated at its exact size. O(r).
fn new_row_len(
    row: &[PropertyCount],
    violates: PropertySet,
    up: bool,
    candidates: PropertySet,
) -> usize {
    if up {
        candidates.len()
    } else {
        row.iter()
            .filter(|c| c.threats > 1 || !violates.contains(c.property))
            .count()
    }
}

impl RefCounts {
    /// Counts one threat in (`up`) or out. O(r + p + k).
    fn threat(&mut self, t: &ThreatScenario, violates: PropertySet, up: bool) {
        bump(&mut self.assets, Some(t.asset.index()), up);
        self.properties(t.asset, violates, up);
        for d in &t.damages {
            bump(&mut self.damages, Some(d.index()), up);
        }
        if let Some(tree) = t.attack_tree {
            bump(&mut self.trees, Some(tree.index()), up);
        }
    }

    /// Counts the properties of one threat in or out. A missing asset slot
    /// is ignored. O(r + p).
    fn properties(&mut self, a: AssetIdx, violates: PropertySet, up: bool) {
        if let Some(row) = self.asset_properties.get_mut(a.index()) {
            count_row(row, violates, up);
        }
    }

    /// How many live threats on `a` violate `p`. O(log r).
    fn property(&self, a: AssetIdx, p: Property) -> u32 {
        self.asset_properties
            .get(a.index())
            .map_or(0, |row| row_count(row, p))
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
/// use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
///
/// let ecu = AssetId::new_v7();
/// project.apply(Op::AddAsset {
///     id: ecu,
///     name: "Brake ECU".into(),
///     properties: m.all_properties(),
///     provenance: Provenance::default(),
/// })?;
///
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let severe = m.level(safety, "severe").ok_or("no level")?;
/// let crash = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id: crash,
///     name: "Vehicle cannot brake".into(),
///     impacts: vec![(safety, severe)],
///     provenance: Provenance::default(),
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
///     provenance: Provenance::default(),
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
    trees: Arena<TreeId, TreeIdx, TreeEntry>,
    controls: Arena<ControlId, ControlIdx, SecurityControl>,
    /// Derived data, rebuilt from `threats` and `trees` (D10).
    refs: RefCounts,
}

/// Checks an entity name. Time O(name length).
fn check_name(kind: EntityKind, name: &str) -> Result<(), ModelError> {
    if name.trim().is_empty() {
        return Err(ModelError::EmptyName { kind });
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ModelError::NameTooLong {
            kind,
            max: MAX_NAME_CHARS,
        });
    }
    Ok(())
}

/// Checks operation provenance. Time O(actor length).
fn check_provenance(provenance: &Provenance) -> Result<(), ModelError> {
    let actor = provenance.actor();
    if !actor.is_empty() {
        if actor.trim().is_empty() {
            return Err(ModelError::EmptyActor);
        }
        if actor.chars().count() > MAX_NAME_CHARS {
            return Err(ModelError::ActorTooLong {
                max: MAX_NAME_CHARS,
            });
        }
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// use aegis_model::{DamageId, EntityKind, Methodology, ModelError, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::with_slot_limit(Arc::new(Methodology::iso21434_r155()?), 1);
    /// let add = |id| Op::AddDamageScenario {
    ///     id,
    ///     name: "d".into(),
    ///     impacts: vec![],
    ///     provenance: Provenance::default(),
    /// };
    /// project.apply(add(DamageId::new_v7()))?;
    /// assert_eq!(
    ///     project.apply(add(DamageId::new_v7())),
    ///     Err(ModelError::ProjectCapacityExceeded {
    ///         kind: EntityKind::DamageScenario,
    ///     }),
    /// );
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
            trees: Arena::new(),
            controls: Arena::new(),
            refs: RefCounts::default(),
        }
    }

    /// Returns the project's methodology. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let (a, b) = (AssetId::new_v7(), AssetId::new_v7());
    /// for id in [b, a] {
    ///     project.apply(Op::AddAsset {
    ///         id,
    ///         name: "ECU".into(),
    ///         properties: m.all_properties(),
    ///         provenance: Provenance::default(),
    ///     })?;
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
    /// # use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id,
    ///     name: "Crash".into(),
    ///     impacts: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id,
    ///     name: "Crash".into(),
    ///     impacts: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id,
    ///     name: "Crash".into(),
    ///     impacts: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset {
    /// #     id: asset, name: "ECU".into(), properties: m.all_properties(),
    /// #     provenance: Provenance::default(),
    /// # })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// let id = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id, name: "Replay".into(), asset, category, catalog_refs: vec![], damages: vec![],
    ///     provenance: Provenance::default(),
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset {
    /// #     id: asset, name: "ECU".into(), properties: m.all_properties(),
    /// #     provenance: Provenance::default(),
    /// # })?;
    /// let category = m.threat_category("denial-of-service").ok_or("none")?;
    /// let id = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id, name: "Flood CAN bus".into(), asset, category, catalog_refs: vec![], damages: vec![],
    ///     provenance: Provenance::default(),
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

    // -- Attack trees ----------------------------------------------------------------

    /// Returns a live attack tree in O(1), or `None` if `idx` is out of
    /// range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TreeId::new_v7();
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("Step"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.attack_tree(idx).map(|t| t.len()), Some(1));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree(&self, idx: TreeIdx) -> Option<&AttackTree> {
        self.trees.get(idx).map(|e| &e.tree)
    }

    /// Returns the provenance of live attack tree `idx` in O(1), or `None`
    /// if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TreeId::new_v7();
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("Step"))?;
    /// let prov = Provenance::ai("tara-assistant", 1_700_000_000_000);
    /// project.apply(Op::AddAttackTree {
    ///     id,
    ///     tree,
    ///     step_feasibility: vec![],
    ///     provenance: prov.clone(),
    /// })?;
    /// let idx = project.attack_tree_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.attack_tree_provenance(idx), Some(&prov));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree_provenance(&self, idx: TreeIdx) -> Option<&Provenance> {
        self.trees.get(idx).map(|e| &e.provenance)
    }

    /// Resolves an attack tree's public ID to its arena index in O(log n).
    /// Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, TreeId};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.attack_tree_idx(&TreeId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree_idx(&self, id: &TreeId) -> Option<TreeIdx> {
        self.trees.idx(id)
    }

    /// Returns a live attack tree's public ID in O(1), or `None` if `idx` is
    /// out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TreeId::new_v7();
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("Step"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.attack_tree_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree_id(&self, idx: TreeIdx) -> Option<TreeId> {
        self.trees.id(idx)
    }

    /// Returns the number of live attack trees in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.attack_tree_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree_count(&self) -> usize {
        self.trees.len()
    }

    /// Iterates live attack trees in deterministic `TreeId` order. Time O(1)
    /// per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TreeId::new_v7();
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("Step"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.attack_trees().map(|(_, id, _)| id).collect::<Vec<_>>(), [id]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn attack_trees(&self) -> impl Iterator<Item = (TreeIdx, TreeId, &AttackTree)> {
        self.trees.iter().map(|(idx, id, e)| (idx, id, &e.tree))
    }

    /// Returns the feasibility rating of `node` in `tree` in O(1), or `None`
    /// if `tree` or `node` is invalid, `node` is a gate, or the step is
    /// unrated. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let level = m.feasibility_level("medium").ok_or("no level")?;
    /// let tree = AttackTree::new(step, Node::step("Step"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id, tree, step_feasibility: vec![(step, level)], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.step_feasibility(idx, root), Some(level));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_feasibility(&self, tree: TreeIdx, node: NodeIdx) -> Option<FeasibilityLevel> {
        self.trees
            .get(tree)?
            .step_feasibility
            .get(node.index())
            .copied()
            .flatten()
    }

    /// Returns the per-node step feasibility slice in `NodeIdx` order in
    /// O(1), or `None` if `tree` is out of range or tombstoned. Gate slots
    /// are always `None`. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let level = m.feasibility_level("high").ok_or("no level")?;
    /// let tree = AttackTree::new(step, Node::step("Step"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id, tree, step_feasibility: vec![(step, level)], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.step_feasibilities(idx), Some(&[Some(level)][..]));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_feasibilities(&self, tree: TreeIdx) -> Option<&[Option<FeasibilityLevel>]> {
        self.trees.get(tree).map(|e| e.step_feasibility.as_slice())
    }

    // -- Security controls -------------------------------------------------------

    /// Returns a live security control in O(1), or `None` if `idx` is out of
    /// range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "Authenticated boot".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// assert_eq!(
    ///     project.security_control(idx).map(|c| c.name()),
    ///     Some("Authenticated boot"),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn security_control(&self, idx: ControlIdx) -> Option<&SecurityControl> {
        self.controls.get(idx)
    }

    /// Resolves a security control's public ID to its arena index in
    /// O(log n). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.security_control_idx(&ControlId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn security_control_idx(&self, id: &ControlId) -> Option<ControlIdx> {
        self.controls.idx(id)
    }

    /// Returns a live security control's public ID in O(1), or `None` if
    /// `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "Authenticated boot".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.security_control_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn security_control_id(&self, idx: ControlIdx) -> Option<ControlId> {
        self.controls.id(idx)
    }

    /// Returns the number of live security controls in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.security_control_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn security_control_count(&self) -> usize {
        self.controls.len()
    }

    /// Iterates live security controls in deterministic `ControlId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (a, b) = (ControlId::new_v7(), ControlId::new_v7());
    /// for id in [b, a] {
    ///     project.apply(Op::AddSecurityControl {
    ///         id,
    ///         name: "Ctrl".into(),
    ///         provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let ids: Vec<_> = project.security_controls().map(|(_, id, _)| id).collect();
    /// assert_eq!(ids, [a.min(b), a.max(b)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn security_controls(
        &self,
    ) -> impl Iterator<Item = (ControlIdx, ControlId, &SecurityControl)> {
        self.controls.iter()
    }

    /// Returns the [`StepMitigation`]s attached to `node` in `tree` in
    /// canonical `ControlId` order in O(1), or `&[]` if `tree` or `node` is
    /// out of range, tombstoned, or has no mitigations. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "Rate limiter".into(), provenance: Provenance::default(),
    /// })?;
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Flood"))?;
    /// let root = tree.root();
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, low)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// assert_eq!(project.step_mitigations(tree_idx, root).len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_mitigations(&self, tree: TreeIdx, node: NodeIdx) -> &[StepMitigation] {
        self.trees
            .get(tree)
            .and_then(|e| e.step_mitigations.get(node.index()))
            .map_or(&[], Box::as_ref)
    }

    /// Returns the residual feasibility of `node` in `tree` — the minimum of
    /// its inherent [`step_feasibility`](Self::step_feasibility) and all
    /// attached [`step_mitigations`](Self::step_mitigations) — or `None` if
    /// `tree` or `node` is invalid, `node` is a gate, or the step has no
    /// inherent feasibility rating.
    ///
    /// Time O(m) where `m <= 16` is the number of mitigations on `node`,
    /// space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "HSM".into(), provenance: Provenance::default(),
    /// })?;
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Extract key"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, high)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// assert_eq!(project.residual_step_feasibility(tree_idx, root), Some(high));
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.residual_step_feasibility(tree_idx, root), Some(low));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn residual_step_feasibility(
        &self,
        tree: TreeIdx,
        node: NodeIdx,
    ) -> Option<FeasibilityLevel> {
        self.trees.get(tree)?.residual_step_feasibility(node)
    }

    /// Evaluates attack tree `id` bottom-up using
    /// [`MaxMin<FeasibilityLevel>`] and its stored step feasibility ratings.
    ///
    /// Time O(V + E), space O(V).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownAttackTree`] if `id` is not a live attack tree in
    /// the project; [`ModelError::MissingLeafValue`] if any step node in the
    /// tree has no feasibility rating; [`ModelError::EmptyGate`] if a gate has
    /// no children; [`ModelError::Cycle`] if the tree contains a cycle.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, GateKind, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = TreeId::new_v7();
    /// let (root, s1, s2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Goal"))?;
    /// tree.add_child(tree.root(), s1, Node::step("Hard"))?;
    /// tree.add_child(tree.root(), s2, Node::step("Easy"))?;
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     id,
    ///     tree,
    ///     step_feasibility: vec![(s1, low), (s2, high)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let eval = project.evaluate_tree(id)?;
    /// assert_eq!(eval.root_value(), &high);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_tree(&self, id: TreeId) -> Result<Evaluation<FeasibilityLevel>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        evaluate(&entry.tree, &MaxMin::<FeasibilityLevel>::new(), |idx| {
            entry.step_feasibility.get(idx.index()).copied().flatten()
        })
    }

    /// Evaluates the attack tree linked to threat scenario `id` using
    /// [`MaxMin<FeasibilityLevel>`].
    ///
    /// Time O(log n + V + E), space O(V).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownThreatScenario`] if `id` is not a live threat
    /// scenario in the project; [`ModelError::NoAttackTree`] if the threat has
    /// no linked attack tree; [`ModelError::MissingLeafValue`] if any step in
    /// the linked tree is unrated; [`ModelError::EmptyGate`] if a gate has no
    /// children; [`ModelError::Cycle`] if the linked tree contains a cycle.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// let tree = AttackTree::new(step, Node::step("Step"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.evaluate_threat(threat)?.root_value(), &med);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_threat(
        &self,
        id: ThreatId,
    ) -> Result<Evaluation<FeasibilityLevel>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.evaluate_tree(tree_id)
    }

    /// Evaluates the risk of threat scenario `id` across all of its linked
    /// damage scenarios and their rated impact categories.
    ///
    /// If the threat has a linked attack tree, its feasibility is evaluated via
    /// [`evaluate_threat`](Self::evaluate_threat) and recorded in
    /// [`ThreatRisk::tree_evaluation`]; otherwise this method falls back to the
    /// threat's [`ThreatScenario::direct_feasibility`] (`tree_evaluation` is
    /// `None`). It then looks up `(impact, feasibility)` in each rated
    /// category's 2D risk matrix ([`Methodology::risk_for`]). Entries are
    /// returned in canonical `(DamageId, ImpactCategory)` order, and
    /// [`ThreatRisk::deciding_entry`] records the first entry in that order
    /// that attains [`ThreatRisk::max_risk`].
    ///
    /// Time O(log n + V + E + d * c), where `d` is the number of linked damage
    /// scenarios (`d <= 256`) and `c` is the number of impact categories
    /// (`c <= 32`). Space O(V + d * c).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownThreatScenario`] if `id` is not a live threat
    /// scenario; [`ModelError::NoAttackTree`] if the threat has neither a
    /// linked attack tree nor a [`ThreatScenario::direct_feasibility`] rating;
    /// or any attack tree evaluation error from
    /// [`evaluate_threat`](Self::evaluate_threat) when a tree is linked.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let safety = m.impact_category("safety").ok_or("no category")?;
    /// let severe = m.level(safety, "severe").ok_or("no level")?;
    /// let damage = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id: damage, name: "Crash".into(), impacts: vec![(safety, severe)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Spoof".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
    /// })?;
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let risk = project.evaluate_risk(threat)?;
    /// assert_eq!(risk.max_risk(), m.risk_level("r5"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_risk(&self, id: ThreatId) -> Result<ThreatRisk, ModelError> {
        let t = self.threat_ref(id)?;
        let (feasibility, tree_evaluation) = if let Some(tree_idx) = t.attack_tree {
            let tree_id = self
                .trees
                .id(tree_idx)
                .ok_or(ModelError::UnknownThreatScenario(id))?;
            let eval = self.evaluate_tree(tree_id)?;
            (*eval.root_value(), Some(eval))
        } else {
            let level = t.direct_feasibility.ok_or(ModelError::NoAttackTree(id))?;
            (level, None)
        };
        Ok(self.risk_from_feasibility(t, feasibility, tree_evaluation))
    }

    /// Evaluates attack tree `id` bottom-up using
    /// [`MaxMin<FeasibilityLevel>`] and each step's
    /// [`residual_step_feasibility`](Self::residual_step_feasibility)
    /// (the minimum of its inherent feasibility and all attached
    /// [`StepMitigation`] caps).
    ///
    /// Comparing [`evaluate_tree`](Self::evaluate_tree) with
    /// `evaluate_residual_tree` reveals whether security controls actually
    /// reduce the root feasibility or whether an unmitigated `OR` branch
    /// circumvents the controls (by inspecting [`Evaluation::why_path`]).
    ///
    /// Time O(V * m + E) where `m <= 16` is the number of mitigations per
    /// step, space O(V).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownAttackTree`] if `id` is not a live attack tree in
    /// the project; [`ModelError::MissingLeafValue`] if any step node in the
    /// tree has no inherent feasibility rating; [`ModelError::EmptyGate`] if a
    /// gate has no children; [`ModelError::Cycle`] if the tree contains a
    /// cycle.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ControlId, GateKind, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let (very_low, med, high) = (
    ///     m.feasibility_level("very-low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let (root, obd, ota) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise ECU"))?;
    /// tree.add_child(tree.root(), obd, Node::step("Via OBD port"))?;
    /// tree.add_child(tree.root(), ota, Node::step("Via OTA update"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id,
    ///     tree,
    ///     step_feasibility: vec![(obd, high), (ota, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "Disable OBD in production".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: obd, control: ctrl, feasibility: Some(very_low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// // Inherent path goes through OBD (high); residual path circumvents it via OTA (medium).
    /// assert_eq!(project.evaluate_tree(tree_id)?.root_value(), &high);
    /// assert_eq!(project.evaluate_residual_tree(tree_id)?.root_value(), &med);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_residual_tree(
        &self,
        id: TreeId,
    ) -> Result<Evaluation<FeasibilityLevel>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        evaluate(&entry.tree, &MaxMin::<FeasibilityLevel>::new(), |idx| {
            entry.residual_step_feasibility(idx)
        })
    }

    /// Evaluates the attack tree linked to threat scenario `id` using
    /// [`evaluate_residual_tree`](Self::evaluate_residual_tree).
    ///
    /// Time O(log n + V * m + E), space O(V).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownThreatScenario`] if `id` is not a live threat
    /// scenario in the project; [`ModelError::NoAttackTree`] if the threat has
    /// no linked attack tree; [`ModelError::MissingLeafValue`] if any step in
    /// the linked tree is unrated; [`ModelError::EmptyGate`] if a gate has no
    /// children; [`ModelError::Cycle`] if the linked tree contains a cycle.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (low, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Replay frame"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "SecOC freshness counter".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.evaluate_residual_threat(threat)?.root_value(), &low);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_residual_threat(
        &self,
        id: ThreatId,
    ) -> Result<Evaluation<FeasibilityLevel>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.evaluate_residual_tree(tree_id)
    }

    /// Evaluates the residual risk of threat scenario `id` across all of its
    /// linked damage scenarios and their rated impact categories, accounting
    /// for all [`StepMitigation`]s attached to the threat's linked attack
    /// tree.
    ///
    /// If the threat has a linked attack tree, its residual feasibility is
    /// evaluated via [`evaluate_residual_tree`](Self::evaluate_residual_tree)
    /// and recorded in [`ThreatRisk::tree_evaluation`]; otherwise this method
    /// falls back to [`ThreatScenario::direct_feasibility`]
    /// (`tree_evaluation` is `None`).
    ///
    /// Time O(log n + V * m + E + d * c), space O(V + d * c).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownThreatScenario`] if `id` is not a live threat
    /// scenario; [`ModelError::NoAttackTree`] if the threat has neither a
    /// linked attack tree nor a [`ThreatScenario::direct_feasibility`] rating;
    /// or any attack tree evaluation error from
    /// [`evaluate_residual_tree`](Self::evaluate_residual_tree) when a tree is
    /// linked.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, ControlId, DamageId, Methodology, Node, NodeId, Op, Project,
    /// #     Provenance, ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let safety = m.impact_category("safety").ok_or("no category")?;
    /// let severe = m.level(safety, "severe").ok_or("no level")?;
    /// let damage = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id: damage, name: "Crash".into(), impacts: vec![(safety, severe)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Spoof".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
    /// })?;
    /// let (very_low, high) = (
    ///     m.feasibility_level("very-low").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "SecOC".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(very_low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.evaluate_risk(threat)?.max_risk(), m.risk_level("r5"));
    /// assert_eq!(project.evaluate_residual_risk(threat)?.max_risk(), m.risk_level("r2"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_residual_risk(&self, id: ThreatId) -> Result<ThreatRisk, ModelError> {
        let t = self.threat_ref(id)?;
        let (feasibility, tree_evaluation) = if let Some(tree_idx) = t.attack_tree {
            let tree_id = self
                .trees
                .id(tree_idx)
                .ok_or(ModelError::UnknownThreatScenario(id))?;
            let eval = self.evaluate_residual_tree(tree_id)?;
            (*eval.root_value(), Some(eval))
        } else {
            let level = t.direct_feasibility.ok_or(ModelError::NoAttackTree(id))?;
            (level, None)
        };
        Ok(self.risk_from_feasibility(t, feasibility, tree_evaluation))
    }

    /// Evaluates attack tree `id` using inherent step feasibility ratings and
    /// returns up to `k` of the easiest minimal attack paths in descending
    /// order of feasibility (rule A4).
    ///
    /// Time O((V + E) + k * V * (V + E) * log(k * d)) on trees where `d` is
    /// the maximum gate arity; space O(V + k * d * V).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate_tree`](Self::evaluate_tree).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, GateKind, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = TreeId::new_v7();
    /// let (root, s1, s2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Goal"))?;
    /// let i1 = tree.add_child(tree.root(), s1, Node::step("Hard"))?;
    /// let i2 = tree.add_child(tree.root(), s2, Node::step("Easy"))?;
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     id,
    ///     tree,
    ///     step_feasibility: vec![(s1, low), (s2, high)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let paths = project.top_k_tree_paths(id, 2)?;
    /// assert_eq!(paths.len(), 2);
    /// assert_eq!(paths[0].steps(), &[i2]);
    /// assert_eq!(paths[1].steps(), &[i1]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn top_k_tree_paths(
        &self,
        id: TreeId,
        k: usize,
    ) -> Result<Vec<AttackPath<FeasibilityLevel>>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let eval = self.evaluate_tree(id)?;
        Ok(eval.top_k_paths(&entry.tree, k).unwrap_or_default())
    }

    /// Evaluates attack tree `id` using residual step feasibility ratings
    /// ([`residual_step_feasibility`](Self::residual_step_feasibility)) and
    /// returns up to `k` of the easiest minimal attack paths in descending
    /// order of residual feasibility (rule A4).
    ///
    /// Time O((V * m + E) + k * V * (V + E) * log(k * d)), space
    /// O(V + k * d * V).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate_residual_tree`](Self::evaluate_residual_tree).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ControlId, GateKind, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let (very_low, med, high) = (
    ///     m.feasibility_level("very-low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let (root, obd, ota) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise"))?;
    /// let i_obd = tree.add_child(tree.root(), obd, Node::step("OBD"))?;
    /// let i_ota = tree.add_child(tree.root(), ota, Node::step("OTA"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id,
    ///     tree,
    ///     step_feasibility: vec![(obd, high), (ota, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "Disable OBD".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: obd, control: ctrl, feasibility: Some(very_low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.top_k_tree_paths(tree_id, 1)?[0].steps(), &[i_obd]);
    /// assert_eq!(project.top_k_residual_tree_paths(tree_id, 1)?[0].steps(), &[i_ota]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn top_k_residual_tree_paths(
        &self,
        id: TreeId,
        k: usize,
    ) -> Result<Vec<AttackPath<FeasibilityLevel>>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let eval = self.evaluate_residual_tree(id)?;
        Ok(eval.top_k_paths(&entry.tree, k).unwrap_or_default())
    }

    /// Evaluates the attack tree linked to threat scenario `id` using inherent
    /// step feasibility ratings and returns up to `k` of the easiest minimal
    /// attack paths in descending order of feasibility (rule A4).
    ///
    /// Time O(log n + (V + E) + k * V * (V + E) * log(k * d)), space
    /// O(V + k * d * V).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate_threat`](Self::evaluate_threat).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Step"))?,
    ///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let paths = project.top_k_threat_paths(threat, 5)?;
    /// assert_eq!(paths.len(), 1);
    /// assert_eq!(*paths[0].feasibility(), med);
    /// # Ok(())
    /// # }
    /// ```
    pub fn top_k_threat_paths(
        &self,
        id: ThreatId,
        k: usize,
    ) -> Result<Vec<AttackPath<FeasibilityLevel>>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.top_k_tree_paths(tree_id, k)
    }

    /// Evaluates the attack tree linked to threat scenario `id` using residual
    /// step feasibility ratings and returns up to `k` of the easiest minimal
    /// attack paths in descending order of residual feasibility (rule A4).
    ///
    /// Time O(log n + (V * m + E) + k * V * (V + E) * log(k * d)), space
    /// O(V + k * d * V).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate_residual_threat`](Self::evaluate_residual_threat).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (low, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Replay"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "SecOC".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let paths = project.top_k_residual_threat_paths(threat, 5)?;
    /// assert_eq!(paths.len(), 1);
    /// assert_eq!(*paths[0].feasibility(), low);
    /// # Ok(())
    /// # }
    /// ```
    pub fn top_k_residual_threat_paths(
        &self,
        id: ThreatId,
        k: usize,
    ) -> Result<Vec<AttackPath<FeasibilityLevel>>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.top_k_residual_tree_paths(tree_id, k)
    }

    /// Combines `feasibility` with every rated `(damage, category, impact)`
    /// linked from `t` in canonical `(DamageId, ImpactCategory)` order.
    /// Time O(d * c), space O(d * c).
    fn risk_from_feasibility(
        &self,
        t: &ThreatScenario,
        feasibility: FeasibilityLevel,
        tree_evaluation: Option<Evaluation<FeasibilityLevel>>,
    ) -> ThreatRisk {
        let mut entries = Vec::new();
        let mut deciding: Option<usize> = None;
        for &damage_idx in &t.damages {
            let (Some(damage_id), Some(damage)) =
                (self.damages.id(damage_idx), self.damages.get(damage_idx))
            else {
                continue;
            };
            for (category, impact) in damage.impacts() {
                let Some(impact) = impact else { continue };
                let Some(risk) = self.methodology.risk_for(category, impact, feasibility) else {
                    continue;
                };
                let is_higher = deciding
                    .and_then(|idx| entries.get(idx))
                    .is_none_or(|best: &ImpactRisk| risk > best.risk);
                if is_higher {
                    deciding = Some(entries.len());
                }
                entries.push(ImpactRisk {
                    damage: damage_id,
                    category,
                    impact,
                    feasibility,
                    risk,
                });
            }
        }
        ThreatRisk {
            feasibility,
            tree_evaluation,
            entries,
            deciding,
        }
    }

    /// Runs completeness and consistency checks over the project and returns
    /// all [`LintFinding`]s in canonical [`LintCode`] order, independent of
    /// operation history (rule P4).
    ///
    /// When the returned slice contains no [`LintSeverity::Error`](crate::LintSeverity::Error)
    /// findings, every attack tree in the project has non-empty gates and
    /// rated leaf steps, and every threat scenario has a valid feasibility
    /// source, so [`evaluate_tree`](Self::evaluate_tree),
    /// [`evaluate_residual_tree`](Self::evaluate_residual_tree),
    /// [`evaluate_risk`](Self::evaluate_risk), and
    /// [`evaluate_residual_risk`](Self::evaluate_residual_risk) are all
    /// guaranteed to succeed (as are [`evaluate_threat`](Self::evaluate_threat)
    /// and [`evaluate_residual_threat`](Self::evaluate_residual_threat) for
    /// every threat with a linked attack tree).
    ///
    /// Time: O(A * P + D + T + K * V + C + F log F), where A is
    /// [`asset_count`](Self::asset_count), P is
    /// [`Methodology::property_count`], D is
    /// [`damage_scenario_count`](Self::damage_scenario_count), T is
    /// [`threat_scenario_count`](Self::threat_scenario_count), K is
    /// [`attack_tree_count`](Self::attack_tree_count), V is the maximum nodes
    /// per attack tree (`<= 32`), C is
    /// [`security_control_count`](Self::security_control_count), and F is the
    /// number of findings returned. Space: O(F).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{
    ///     AssetId, ControlId, DamageId, LintCode, LintFinding, Methodology, Op, Project,
    ///     Provenance, RiskTreatment, ThreatId,
    /// };
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// assert!(project.lint().is_empty());
    ///
    /// let asset = AssetId::new_v7();
    /// let integrity = m.property("integrity").ok_or("no prop")?;
    /// project.apply(Op::AddAsset {
    ///     id: asset,
    ///     name: "ECU".into(),
    ///     properties: [integrity].into_iter().collect(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(
    ///     project.lint(),
    ///     vec![LintFinding::new(LintCode::AssetWithoutThreats { asset })]
    /// );
    ///
    /// // Completing the threat, damage, direct feasibility, and treatment
    /// // clears all lint findings.
    /// let damage = DamageId::new_v7();
    /// let safety = m.impact_category("safety").ok_or("no cat")?;
    /// let severe = m.level(safety, "severe").ok_or("no level")?;
    /// project.apply(Op::AddDamageScenario {
    ///     id: damage,
    ///     name: "Loss of braking".into(),
    ///     impacts: vec![(safety, severe)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let threat = ThreatId::new_v7();
    /// let tampering = m.threat_category("tampering").ok_or("no cat")?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat,
    ///     name: "Reflash".into(),
    ///     asset,
    ///     category: tampering,
    ///     catalog_refs: vec![],
    ///     damages: vec![damage],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let high = m.feasibility_level("high").ok_or("no feas")?;
    /// project.apply(Op::SetThreatFeasibility {
    ///     threat,
    ///     level: Some(high),
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetRiskTreatment {
    ///     threat,
    ///     treatment: Some(RiskTreatment::Avoid),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert!(project.lint().is_empty());
    ///
    /// // An unused security control triggers a warning.
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl,
    ///     name: "Firewall".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(
    ///     project.lint(),
    ///     vec![LintFinding::new(LintCode::SecurityControlUnused { control: ctrl })]
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn lint(&self) -> Vec<LintFinding> {
        let mut findings = Vec::new();
        self.lint_assets(&mut findings);
        self.lint_damages(&mut findings);
        self.lint_threats(&mut findings);
        self.lint_trees(&mut findings);
        self.lint_controls(&mut findings);
        findings.sort_unstable();
        findings
    }

    fn lint_assets(&self, findings: &mut Vec<LintFinding>) {
        for (idx, id, asset) in self.assets.iter() {
            let threats = self.refs.assets.get(idx.index()).copied().unwrap_or(0);
            if threats == 0 {
                findings.push(LintFinding::new(LintCode::AssetWithoutThreats {
                    asset: id,
                }));
            } else {
                let row = self
                    .refs
                    .asset_properties
                    .get(idx.index())
                    .map_or(&[][..], Box::as_ref);
                for property in asset.properties().iter() {
                    if row_count(row, property) == 0 {
                        findings.push(LintFinding::new(LintCode::AssetPropertyUntested {
                            asset: id,
                            property,
                        }));
                    }
                }
            }
        }
    }

    fn lint_damages(&self, findings: &mut Vec<LintFinding>) {
        for (idx, id, damage) in self.damages.iter() {
            if damage.impacts().all(|(_, level)| level.is_none()) {
                findings.push(LintFinding::new(LintCode::DamageScenarioWithoutImpacts {
                    damage: id,
                }));
            }
            if self.refs.damages.get(idx.index()).copied().unwrap_or(0) == 0 {
                findings.push(LintFinding::new(LintCode::DamageScenarioUnlinked {
                    damage: id,
                }));
            }
        }
    }

    fn lint_threats(&self, findings: &mut Vec<LintFinding>) {
        for (_idx, id, threat) in self.threats.iter() {
            if threat.damages().is_empty() {
                findings.push(LintFinding::new(LintCode::ThreatWithoutDamages {
                    threat: id,
                }));
            }
            match (threat.attack_tree(), threat.direct_feasibility()) {
                (None, None) => {
                    findings.push(LintFinding::new(LintCode::ThreatWithoutFeasibility {
                        threat: id,
                    }));
                }
                (Some(_), Some(_)) => {
                    findings.push(LintFinding::new(LintCode::RedundantDirectFeasibility {
                        threat: id,
                    }));
                }
                (Some(_), None) | (None, Some(_)) => {}
            }
            match threat.treatment() {
                None => {
                    findings.push(LintFinding::new(LintCode::ThreatWithoutTreatment {
                        threat: id,
                    }));
                }
                Some(RiskTreatment::Reduce) => {
                    let has_mitigations = threat
                        .attack_tree()
                        .and_then(|t_idx| self.trees.get(t_idx))
                        .is_some_and(|entry| entry.step_mitigations.iter().any(|m| !m.is_empty()));
                    if !has_mitigations {
                        findings.push(LintFinding::new(LintCode::ReduceTreatmentWithoutControls {
                            threat: id,
                        }));
                    }
                }
                Some(RiskTreatment::Avoid | RiskTreatment::Share | RiskTreatment::Retain) => {}
            }
        }
    }

    fn lint_trees(&self, findings: &mut Vec<LintFinding>) {
        for (idx, id, entry) in self.trees.iter() {
            if self.refs.trees.get(idx.index()).copied().unwrap_or(0) == 0 {
                findings.push(LintFinding::new(LintCode::AttackTreeUnlinked { tree: id }));
            }
            for (node_idx, node_id, node) in entry.tree.iter() {
                match node {
                    Node::Gate { .. } => {
                        if entry.tree.children(node_idx).is_empty() {
                            findings.push(LintFinding::new(LintCode::AttackTreeGateEmpty {
                                tree: id,
                                node: node_id,
                            }));
                        }
                    }
                    Node::Step { .. } => {
                        if entry
                            .step_feasibility
                            .get(node_idx.index())
                            .copied()
                            .flatten()
                            .is_none()
                        {
                            findings.push(LintFinding::new(LintCode::AttackTreeStepUnrated {
                                tree: id,
                                node: node_id,
                            }));
                        }
                    }
                }
            }
        }
    }

    fn lint_controls(&self, findings: &mut Vec<LintFinding>) {
        for (idx, id, _control) in self.controls.iter() {
            if self.refs.controls.get(idx.index()).copied().unwrap_or(0) == 0 {
                findings.push(LintFinding::new(LintCode::SecurityControlUnused {
                    control: id,
                }));
            }
        }
    }

    // -- Diff and impact analysis (rule A7) ---------------------------------------------

    /// Computes a deterministic structural diff and change impact report
    /// between `self` (the baseline project) and `other` (the current
    /// project) (rule A7).
    ///
    /// Reports added, removed, and modified entity IDs in canonical ascending
    /// order for every entity kind, together with [`ThreatRiskDelta`] entries
    /// for all threat scenarios affected by any change (including threats
    /// whose linked asset, damage scenario, attack tree, or security control
    /// changed).
    ///
    /// Time O(N + L + `T_aff` * (V * m + E + d * c)) where `N` is the total
    /// number of entities across both projects, `L` is the number of links and
    /// catalog refs, and `T_aff` is the number of affected threat scenarios.
    /// Space O(D) where `D` is the number of changed entities and affected
    /// threats.
    ///
    /// # Errors
    ///
    /// [`ModelError::MethodologyMismatch`] if `self` and `other` do not use
    /// equal methodologies.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, ControlId, DamageId, Methodology, Op, Project, Provenance, RiskChange,
    /// #     ThreatId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut base = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// base.apply(Op::AddAsset {
    ///     id: asset, name: "Gateway".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let safety = m.impact_category("safety").ok_or("no cat")?;
    /// let moderate = m.level(safety, "moderate").ok_or("no level")?;
    /// let severe = m.level(safety, "severe").ok_or("no level")?;
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// let damage = DamageId::new_v7();
    /// base.apply(Op::AddDamageScenario {
    ///     id: damage, name: "Loss of braking".into(), impacts: vec![(safety, moderate)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let cat = m.threat_categories().next().ok_or("no cat")?;
    /// let threat = ThreatId::new_v7();
    /// base.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Inject frame".into(), asset, category: cat,
    ///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
    /// })?;
    /// base.apply(Op::SetThreatFeasibility {
    ///     threat, level: Some(high), provenance: Provenance::default(),
    /// })?;
    ///
    /// // Raise damage impact in `curr` without touching the threat scenario directly.
    /// let mut curr = base.clone();
    /// curr.apply(Op::SetImpact {
    ///     id: damage, category: safety, level: Some(severe), provenance: Provenance::default(),
    /// })?;
    ///
    /// let diff = base.diff(&curr)?;
    /// assert_eq!(diff.damage_scenarios().modified(), &[damage]);
    /// assert!(diff.threat_scenarios().is_empty());
    /// assert_eq!(diff.risk_deltas().len(), 1);
    /// assert_eq!(diff.risk_deltas()[0].threat(), threat);
    /// assert_eq!(diff.risk_deltas()[0].inherent().change(), RiskChange::Increased);
    /// # Ok(())
    /// # }
    /// ```
    pub fn diff(&self, other: &Self) -> Result<ProjectDiff, ModelError> {
        let same_methodology = Arc::ptr_eq(&self.methodology, &other.methodology)
            || *self.methodology == *other.methodology;
        if !same_methodology {
            return Err(ModelError::MethodologyMismatch {
                expected: self.methodology.key().into(),
                expected_version: self.methodology.version().into(),
                found: other.methodology.key().into(),
                found_version: other.methodology.version().into(),
            });
        }
        let assets = diff_arena(&self.assets, &other.assets, |x, y| x == y);
        let damage_scenarios = diff_arena(&self.damages, &other.damages, |x, y| x == y);
        let security_controls = diff_arena(&self.controls, &other.controls, |x, y| x == y);
        let attack_trees = diff_arena(&self.trees, &other.trees, |x, y| {
            self.same_tree(x, other, y)
        });
        let threat_scenarios = diff_arena(&self.threats, &other.threats, |x, y| {
            self.same_threat(x, other, y)
        });
        let risk_deltas = self.diff_risk_deltas(
            other,
            &assets,
            &damage_scenarios,
            &threat_scenarios,
            &attack_trees,
            &security_controls,
        );
        Ok(ProjectDiff {
            assets,
            damage_scenarios,
            threat_scenarios,
            attack_trees,
            security_controls,
            risk_deltas,
        })
    }

    fn diff_risk_deltas(
        &self,
        other: &Self,
        assets: &EntityDiff<AssetId>,
        damages: &EntityDiff<DamageId>,
        threats: &EntityDiff<ThreatId>,
        trees: &EntityDiff<TreeId>,
        controls: &EntityDiff<ControlId>,
    ) -> Box<[ThreatRiskDelta]> {
        let mut deltas = Vec::new();
        let mut base_iter = self.threats.iter().peekable();
        let mut curr_iter = other.threats.iter().peekable();
        loop {
            match (base_iter.peek().copied(), curr_iter.peek().copied()) {
                (Some((_, id_a, _)), Some((_, id_b, _))) => match id_a.cmp(&id_b) {
                    std::cmp::Ordering::Less => {
                        base_iter.next();
                        let (inh, res) = self.threat_risk_snapshots(id_a);
                        deltas.push(ThreatRiskDelta::new(
                            id_a,
                            RiskDelta::new(inh, None),
                            RiskDelta::new(res, None),
                        ));
                    }
                    std::cmp::Ordering::Greater => {
                        curr_iter.next();
                        let (inh, res) = other.threat_risk_snapshots(id_b);
                        deltas.push(ThreatRiskDelta::new(
                            id_b,
                            RiskDelta::new(None, inh),
                            RiskDelta::new(None, res),
                        ));
                    }
                    std::cmp::Ordering::Equal => {
                        let Some((_, _, t_a)) = base_iter.next() else {
                            break;
                        };
                        curr_iter.next();
                        if self.is_threat_affected(
                            id_a, t_a, assets, damages, threats, trees, controls,
                        ) {
                            let (inh_a, res_a) = self.threat_risk_snapshots(id_a);
                            let (inh_b, res_b) = other.threat_risk_snapshots(id_a);
                            deltas.push(ThreatRiskDelta::new(
                                id_a,
                                RiskDelta::new(inh_a, inh_b),
                                RiskDelta::new(res_a, res_b),
                            ));
                        }
                    }
                },
                (Some((_, id_a, _)), None) => {
                    base_iter.next();
                    let (inh, res) = self.threat_risk_snapshots(id_a);
                    deltas.push(ThreatRiskDelta::new(
                        id_a,
                        RiskDelta::new(inh, None),
                        RiskDelta::new(res, None),
                    ));
                }
                (None, Some((_, id_b, _))) => {
                    curr_iter.next();
                    let (inh, res) = other.threat_risk_snapshots(id_b);
                    deltas.push(ThreatRiskDelta::new(
                        id_b,
                        RiskDelta::new(None, inh),
                        RiskDelta::new(None, res),
                    ));
                }
                (None, None) => break,
            }
        }
        deltas.into_boxed_slice()
    }

    #[allow(clippy::too_many_arguments)]
    fn is_threat_affected(
        &self,
        id: ThreatId,
        t: &ThreatScenario,
        assets: &EntityDiff<AssetId>,
        damages: &EntityDiff<DamageId>,
        threats: &EntityDiff<ThreatId>,
        trees: &EntityDiff<TreeId>,
        controls: &EntityDiff<ControlId>,
    ) -> bool {
        if threats.modified().binary_search(&id).is_ok() {
            return true;
        }
        if self
            .assets
            .id(t.asset)
            .is_some_and(|a| assets.modified().binary_search(&a).is_ok())
        {
            return true;
        }
        if t.damages
            .iter()
            .filter_map(|&d| self.damages.id(d))
            .any(|d| damages.modified().binary_search(&d).is_ok())
        {
            return true;
        }
        if let Some(tree_idx) = t.attack_tree {
            if self
                .trees
                .id(tree_idx)
                .is_some_and(|tr| trees.modified().binary_search(&tr).is_ok())
            {
                return true;
            }
            if !controls.modified().is_empty()
                && let Some(entry) = self.trees.get(tree_idx)
                && entry
                    .step_mitigations
                    .iter()
                    .flat_map(|ms| ms.iter())
                    .filter_map(|m| self.controls.id(m.control))
                    .any(|c| controls.modified().binary_search(&c).is_ok())
            {
                return true;
            }
        }
        false
    }

    fn threat_risk_snapshots(&self, id: ThreatId) -> (Option<RiskSnapshot>, Option<RiskSnapshot>) {
        let inh = self
            .evaluate_risk(id)
            .ok()
            .map(|r| RiskSnapshot::new(r.feasibility(), r.max_risk()));
        let res = self
            .evaluate_residual_risk(id)
            .ok()
            .map(|r| RiskSnapshot::new(r.feasibility(), r.max_risk()));
        (inh, res)
    }
}

fn diff_arena<Id: Copy + Ord, Idx: ArenaIndex, T>(
    base: &Arena<Id, Idx, T>,
    curr: &Arena<Id, Idx, T>,
    mut eq: impl FnMut(&T, &T) -> bool,
) -> EntityDiff<Id> {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    let mut base_iter = base.iter().peekable();
    let mut curr_iter = curr.iter().peekable();
    loop {
        match (base_iter.peek().copied(), curr_iter.peek().copied()) {
            (Some((_, id_a, _)), Some((_, id_b, _))) => match id_a.cmp(&id_b) {
                std::cmp::Ordering::Less => {
                    base_iter.next();
                    removed.push(id_a);
                }
                std::cmp::Ordering::Greater => {
                    curr_iter.next();
                    added.push(id_b);
                }
                std::cmp::Ordering::Equal => {
                    let Some((_, _, v_a)) = base_iter.next() else {
                        break;
                    };
                    let Some((_, _, v_b)) = curr_iter.next() else {
                        break;
                    };
                    if !eq(v_a, v_b) {
                        modified.push(id_a);
                    }
                }
            },
            (Some((_, id_a, _)), None) => {
                base_iter.next();
                removed.push(id_a);
            }
            (None, Some((_, id_b, _))) => {
                curr_iter.next();
                added.push(id_b);
            }
            (None, None) => break,
        }
    }
    EntityDiff {
        added: added.into_boxed_slice(),
        removed: removed.into_boxed_slice(),
        modified: modified.into_boxed_slice(),
    }
}

impl Project {
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
    /// refs, at most 256 damage links, each existing and unique);
    /// referential rules (no removal or property drop while referenced,
    /// links and refs must or must not exist, a link must not be the
    /// 257th); provenance; and capacity. See [`ModelError`] for each
    /// variant.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, ModelError, Op, Project, PropertySet, Provenance};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = AssetId::new_v7();
    /// let op = Op::AddAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     properties: PropertySet::empty(),
    ///     provenance: Provenance::default(),
    /// };
    /// assert_eq!(project.validate(&op), Err(ModelError::NoSecurityProperties(id)));
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_lines)]
    pub fn validate(&self, op: &Op) -> Result<(), ModelError> {
        match op {
            Op::AddAsset {
                id,
                name,
                properties,
                provenance,
            } => return self.check_add_asset(*id, name, *properties, provenance),
            Op::RenameAsset { id, name, .. } => {
                self.live_asset(*id)?;
                check_name(EntityKind::Asset, name)?;
            }
            Op::SetAssetProperties { id, properties, .. } => {
                self.check_set_asset_properties(*id, *properties)?;
            }
            Op::RemoveAsset { id } => return self.check_remove_asset(*id),
            Op::AddDamageScenario {
                id,
                name,
                impacts,
                provenance,
            } => return self.check_add_damage(*id, name, impacts, provenance),
            Op::RenameDamageScenario { id, name, .. } => {
                self.live_damage(*id)?;
                check_name(EntityKind::DamageScenario, name)?;
            }
            Op::SetImpact {
                id,
                category,
                level,
                ..
            } => {
                self.live_damage(*id)?;
                self.check_rating(*category, *level)?;
            }
            Op::RemoveDamageScenario { id } => return self.check_remove_damage(*id),
            Op::AddThreatScenario {
                id,
                name,
                asset,
                category,
                catalog_refs,
                damages,
                provenance,
            } => {
                return self.check_add_threat(
                    *id,
                    name,
                    *asset,
                    *category,
                    catalog_refs,
                    damages,
                    provenance,
                );
            }
            Op::RenameThreatScenario { id, name, .. } => {
                self.live_threat(*id)?;
                check_name(EntityKind::ThreatScenario, name)?;
            }
            Op::SetThreatCategory { id, category, .. } => {
                let t = self.threat_ref(*id)?;
                let (asset_id, asset) = self.threat_asset(*id, t)?;
                self.check_category_fits(asset_id, asset, *category)?;
            }
            Op::AddCatalogRef {
                threat,
                catalog_ref,
                ..
            } => self.check_catalog_ref_change(*threat, catalog_ref, true)?,
            Op::RemoveCatalogRef {
                threat,
                catalog_ref,
                ..
            } => self.check_catalog_ref_change(*threat, catalog_ref, false)?,
            Op::LinkDamage { threat, damage, .. } => self.check_link(*threat, *damage, true)?,
            Op::UnlinkDamage { threat, damage, .. } => self.check_link(*threat, *damage, false)?,
            Op::RemoveThreatScenario { id } => return self.check_remove_threat(*id),
            Op::AddAttackTree {
                id,
                tree,
                step_feasibility,
                provenance,
            } => return self.check_add_tree(*id, tree, step_feasibility, provenance),
            Op::SetStepFeasibility {
                tree, node, level, ..
            } => self.check_set_step_feasibility(*tree, *node, *level)?,
            Op::SetThreatFeasibility { threat, level, .. } => {
                self.check_set_threat_feasibility(*threat, *level)?;
            }
            Op::SetThreatAttackTree { threat, tree, .. } => {
                self.check_set_threat_attack_tree(*threat, *tree)?;
            }
            Op::RemoveAttackTree { id } => return self.check_remove_tree(*id),
            Op::SetRiskTreatment { threat, .. } => {
                self.live_threat(*threat)?;
            }
            Op::AddSecurityControl {
                id,
                name,
                provenance,
            } => return self.check_add_control(*id, name, provenance),
            Op::RenameSecurityControl { id, name, .. } => {
                self.live_control(*id)?;
                check_name(EntityKind::SecurityControl, name)?;
            }
            Op::RemoveSecurityControl { id } => return self.check_remove_control(*id),
            Op::SetStepMitigation {
                tree,
                node,
                control,
                feasibility,
                ..
            } => self.check_set_step_mitigation(*tree, *node, *control, *feasibility)?,
        }
        op.provenance().map_or(Ok(()), check_provenance)
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
    /// use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = DamageId::new_v7();
    /// let prov = Provenance::human("alice", 10);
    /// project.apply(Op::AddDamageScenario {
    ///     id,
    ///     name: "Fraud".into(),
    ///     impacts: vec![],
    ///     provenance: prov.clone(),
    /// })?;
    /// let before = project.clone();
    /// let cat = m.impact_category("financial").ok_or("none")?;
    /// let level = m.level(cat, "major");
    /// let undo = project.apply(Op::SetImpact {
    ///     id,
    ///     category: cat,
    ///     level,
    ///     provenance: Provenance::human("bob", 20),
    /// })?;
    /// assert_eq!(
    ///     undo,
    ///     Op::SetImpact {
    ///         id,
    ///         category: cat,
    ///         level: None,
    ///         provenance: prov,
    ///     }
    /// );
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
    #[allow(clippy::too_many_lines)]
    fn commit(&mut self, op: Op) -> Result<Op, ModelError> {
        match op {
            Op::AddAsset {
                id,
                name,
                properties,
                provenance,
            } => self.commit_add_asset(id, name, properties, provenance),
            Op::RenameAsset {
                id,
                name,
                provenance,
            } => self.commit_rename_asset(id, name, provenance),
            Op::SetAssetProperties {
                id,
                properties,
                provenance,
            } => self.commit_set_asset_properties(id, properties, provenance),
            Op::RemoveAsset { id } => self.commit_remove_asset(id),
            Op::AddDamageScenario {
                id,
                name,
                impacts,
                provenance,
            } => self.commit_add_damage(id, name, &impacts, provenance),
            Op::RenameDamageScenario {
                id,
                name,
                provenance,
            } => self.commit_rename_damage(id, name, provenance),
            Op::SetImpact {
                id,
                category,
                level,
                provenance,
            } => self.commit_set_impact(id, category, level, provenance),
            Op::RemoveDamageScenario { id } => self.commit_remove_damage(id),
            Op::AddThreatScenario {
                id,
                name,
                asset,
                category,
                catalog_refs,
                damages,
                provenance,
            } => {
                self.commit_add_threat(id, name, asset, category, catalog_refs, damages, provenance)
            }
            Op::RenameThreatScenario {
                id,
                name,
                provenance,
            } => self.commit_rename_threat(id, name, provenance),
            Op::SetThreatCategory {
                id,
                category,
                provenance,
            } => self.commit_set_category(id, category, provenance),
            Op::AddCatalogRef {
                threat,
                catalog_ref,
                provenance,
            } => self.commit_catalog_ref(threat, catalog_ref, provenance, true),
            Op::RemoveCatalogRef {
                threat,
                catalog_ref,
                provenance,
            } => self.commit_catalog_ref(threat, catalog_ref, provenance, false),
            Op::LinkDamage {
                threat,
                damage,
                provenance,
            } => self.commit_link(threat, damage, provenance, true),
            Op::UnlinkDamage {
                threat,
                damage,
                provenance,
            } => self.commit_link(threat, damage, provenance, false),
            Op::RemoveThreatScenario { id } => self.commit_remove_threat(id),
            Op::AddAttackTree {
                id,
                tree,
                step_feasibility,
                provenance,
            } => self.commit_add_tree(id, tree, step_feasibility, provenance),
            Op::SetStepFeasibility {
                tree,
                node,
                level,
                provenance,
            } => self.commit_set_step_feasibility(tree, node, level, provenance),
            Op::SetThreatFeasibility {
                threat,
                level,
                provenance,
            } => self.commit_set_threat_feasibility(threat, level, provenance),
            Op::SetThreatAttackTree {
                threat,
                tree,
                provenance,
            } => self.commit_set_threat_attack_tree(threat, tree, provenance),
            Op::RemoveAttackTree { id } => self.commit_remove_tree(id),
            Op::SetRiskTreatment {
                threat,
                treatment,
                provenance,
            } => self.commit_set_risk_treatment(threat, treatment, provenance),
            Op::AddSecurityControl {
                id,
                name,
                provenance,
            } => self.commit_add_control(id, name, provenance),
            Op::RenameSecurityControl {
                id,
                name,
                provenance,
            } => self.commit_rename_control(id, name, provenance),
            Op::RemoveSecurityControl { id } => self.commit_remove_control(id),
            Op::SetStepMitigation {
                tree,
                node,
                control,
                feasibility,
                provenance,
            } => self.commit_set_step_mitigation(tree, node, control, feasibility, provenance),
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

    fn tree_mut(&mut self, id: TreeId) -> Result<&mut TreeEntry, ModelError> {
        let idx = self.tree_index(id)?;
        self.trees
            .get_mut(idx)
            .ok_or(ModelError::UnknownAttackTree(id))
    }

    fn control_mut(&mut self, id: ControlId) -> Result<&mut SecurityControl, ModelError> {
        let idx = self.control_index(id)?;
        self.controls
            .get_mut(idx)
            .ok_or(ModelError::UnknownSecurityControl(id))
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

    fn tree_index(&self, id: TreeId) -> Result<TreeIdx, ModelError> {
        self.trees.idx(&id).ok_or(ModelError::UnknownAttackTree(id))
    }

    fn control_index(&self, id: ControlId) -> Result<ControlIdx, ModelError> {
        self.controls
            .idx(&id)
            .ok_or(ModelError::UnknownSecurityControl(id))
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

    fn live_tree(&self, id: TreeId) -> Result<(TreeIdx, &TreeEntry), ModelError> {
        let idx = self.tree_index(id)?;
        let e = self
            .trees
            .get(idx)
            .ok_or(ModelError::UnknownAttackTree(id))?;
        Ok((idx, e))
    }

    fn live_control(&self, id: ControlId) -> Result<ControlIdx, ModelError> {
        let idx = self.control_index(id)?;
        self.controls
            .get(idx)
            .map(|_| idx)
            .ok_or(ModelError::UnknownSecurityControl(id))
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

    /// The public ID of a linked attack tree, if any.
    fn tree_key(&self, idx: Option<TreeIdx>) -> Option<TreeId> {
        idx.and_then(|i| self.trees.id(i))
    }

    /// The public ID of a referenced security control. Referenced security
    /// controls are always live.
    fn control_key(&self, idx: ControlIdx) -> Option<ControlId> {
        self.controls.id(idx)
    }

    /// Finds `control` in a step's sorted mitigations: `Ok(pos)` if attached,
    /// `Err(pos)` for the insertion point. O(log m).
    fn find_mitigation(&self, row: &[StepMitigation], control: ControlId) -> Result<usize, usize> {
        row.binary_search_by(|m| self.control_key(m.control).cmp(&Some(control)))
    }

    /// How many live threats on `a` violate `p`. O(log p).
    fn asset_property_refs(&self, a: AssetIdx, p: Property) -> u32 {
        self.refs.property(a, p)
    }

    // -- Checks ----------------------------------------------------------------------

    fn check_add_asset(
        &self,
        id: AssetId,
        name: &str,
        properties: PropertySet,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.assets.idx(&id).is_some() {
            return Err(ModelError::DuplicateAsset(id));
        }
        check_name(EntityKind::Asset, name)?;
        self.methodology.check_properties(properties)?;
        if properties.is_empty() {
            return Err(ModelError::NoSecurityProperties(id));
        }
        check_provenance(provenance)?;
        if self.assets.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Asset,
            });
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
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.damages.idx(&id).is_some() {
            return Err(ModelError::DuplicateDamageScenario(id));
        }
        check_name(EntityKind::DamageScenario, name)?;
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
        check_provenance(provenance)?;
        if self.damages.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::DamageScenario,
            });
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
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.threats.idx(&id).is_some() {
            return Err(ModelError::DuplicateThreatScenario(id));
        }
        check_name(EntityKind::ThreatScenario, name)?;
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
        if let Some(w) = refs.windows(2).find(|w| w.first() == w.get(1)) {
            if let Some(&catalog_ref) = w.first() {
                return Err(ModelError::DuplicateCatalogRef {
                    threat: id,
                    catalog_ref,
                });
            }
        }
        if damages.len() > MAX_DAMAGE_LINKS {
            return Err(ModelError::TooManyDamageLinks {
                threat: id,
                max: MAX_DAMAGE_LINKS,
            });
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
        check_provenance(provenance)?;
        if self.threats.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::ThreatScenario,
            });
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
            (true, true) => Err(ModelError::DuplicateCatalogRef {
                threat,
                catalog_ref: *r,
            }),
            (true, false) if t.catalog_refs.len() >= MAX_CATALOG_REFS => {
                Err(ModelError::TooManyCatalogRefs {
                    threat,
                    max: MAX_CATALOG_REFS,
                })
            }
            (false, false) => Err(ModelError::CatalogRefNotFound {
                threat,
                catalog_ref: *r,
            }),
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
            (true, false) if t.damages.len() >= MAX_DAMAGE_LINKS => {
                Err(ModelError::TooManyDamageLinks {
                    threat,
                    max: MAX_DAMAGE_LINKS,
                })
            }
            (false, false) => Err(ModelError::DamageNotLinked { threat, damage }),
            _ => Ok(()),
        }
    }

    fn check_remove_threat(&self, id: ThreatId) -> Result<(), ModelError> {
        let t = self.threat_ref(id)?;
        if let Some(tree_idx) = t.attack_tree {
            let tree = self
                .trees
                .id(tree_idx)
                .ok_or(ModelError::UnknownThreatScenario(id))?;
            return Err(ModelError::ThreatHasAttackTree { threat: id, tree });
        }
        if let Some(feasibility) = t.direct_feasibility {
            return Err(ModelError::ThreatHasDirectFeasibility {
                threat: id,
                feasibility,
            });
        }
        if let Some(treatment) = t.treatment {
            return Err(ModelError::ThreatHasRiskTreatment {
                threat: id,
                treatment,
            });
        }
        Ok(())
    }

    fn check_add_tree(
        &self,
        id: TreeId,
        tree: &AttackTree,
        step_feasibility: &[(NodeId, FeasibilityLevel)],
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.trees.idx(&id).is_some() {
            return Err(ModelError::DuplicateAttackTree(id));
        }
        if tree.len() > MAX_ATTACK_TREE_NODES {
            return Err(ModelError::TooManyAttackTreeNodes {
                tree: id,
                max: MAX_ATTACK_TREE_NODES,
            });
        }
        for (_, _, node) in tree.iter() {
            check_name(EntityKind::TreeNode, node.label())?;
        }
        tree.topological_order()?;
        let mut seen = vec![false; tree.len()];
        for &(node, level) in step_feasibility {
            let node_idx = tree
                .idx_of(&node)
                .ok_or(ModelError::UnknownTreeNode { tree: id, node })?;
            if !matches!(tree.get(node_idx), Some(Node::Step { .. })) {
                return Err(ModelError::NotAStep { tree: id, node });
            }
            self.methodology.check_feasibility_level(level)?;
            let slot = seen
                .get_mut(node_idx.index())
                .ok_or(ModelError::UnknownTreeNode { tree: id, node })?;
            if *slot {
                return Err(ModelError::DuplicateStepFeasibility { tree: id, node });
            }
            *slot = true;
        }
        check_provenance(provenance)?;
        if self.trees.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::AttackTree,
            });
        }
        Ok(())
    }

    fn check_set_step_feasibility(
        &self,
        tree: TreeId,
        node: NodeId,
        level: Option<FeasibilityLevel>,
    ) -> Result<(), ModelError> {
        let (_, entry) = self.live_tree(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        if !matches!(entry.tree.get(node_idx), Some(Node::Step { .. })) {
            return Err(ModelError::NotAStep { tree, node });
        }
        if let Some(level) = level {
            self.methodology.check_feasibility_level(level)?;
        }
        Ok(())
    }

    fn check_set_threat_feasibility(
        &self,
        threat: ThreatId,
        level: Option<FeasibilityLevel>,
    ) -> Result<(), ModelError> {
        self.live_threat(threat)?;
        if let Some(level) = level {
            self.methodology.check_feasibility_level(level)?;
        }
        Ok(())
    }

    fn check_set_threat_attack_tree(
        &self,
        threat: ThreatId,
        tree: Option<TreeId>,
    ) -> Result<(), ModelError> {
        self.live_threat(threat)?;
        if let Some(tree) = tree {
            self.live_tree(tree)?;
        }
        Ok(())
    }

    fn check_remove_tree(&self, id: TreeId) -> Result<(), ModelError> {
        let (idx, entry) = self.live_tree(id)?;
        let threats = self.refs.trees.get(idx.index()).copied().unwrap_or(0);
        if threats > 0 {
            return Err(ModelError::AttackTreeInUse { tree: id, threats });
        }
        let mitigations: usize = entry.step_mitigations.iter().map(|m| m.len()).sum();
        if mitigations > 0 {
            return Err(ModelError::AttackTreeHasMitigations {
                tree: id,
                mitigations,
            });
        }
        Ok(())
    }

    fn check_add_control(
        &self,
        id: ControlId,
        name: &str,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.controls.idx(&id).is_some() {
            return Err(ModelError::DuplicateSecurityControl(id));
        }
        check_name(EntityKind::SecurityControl, name)?;
        check_provenance(provenance)?;
        if self.controls.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::SecurityControl,
            });
        }
        Ok(())
    }

    fn check_remove_control(&self, id: ControlId) -> Result<(), ModelError> {
        let idx = self.live_control(id)?;
        let steps = self.refs.controls.get(idx.index()).copied().unwrap_or(0);
        if steps > 0 {
            return Err(ModelError::SecurityControlInUse { control: id, steps });
        }
        Ok(())
    }

    fn check_set_step_mitigation(
        &self,
        tree: TreeId,
        node: NodeId,
        control: ControlId,
        feasibility: Option<FeasibilityLevel>,
    ) -> Result<(), ModelError> {
        let (_, entry) = self.live_tree(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        if !matches!(entry.tree.get(node_idx), Some(Node::Step { .. })) {
            return Err(ModelError::NotAStep { tree, node });
        }
        self.live_control(control)?;
        if let Some(level) = feasibility {
            self.methodology.check_feasibility_level(level)?;
            let row = entry
                .step_mitigations
                .get(node_idx.index())
                .ok_or(ModelError::UnknownTreeNode { tree, node })?;
            if self.find_mitigation(row, control).is_err() && row.len() >= MAX_STEP_MITIGATIONS {
                return Err(ModelError::TooManyStepMitigations {
                    tree,
                    node,
                    max: MAX_STEP_MITIGATIONS,
                });
            }
        }
        Ok(())
    }

    // -- Commits -----------------------------------------------------------------------

    fn commit_add_asset(
        &mut self,
        id: AssetId,
        name: Arc<str>,
        properties: PropertySet,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let asset = Asset {
            name,
            properties,
            provenance,
        };
        self.assets.push(id, asset, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Asset,
            },
        )?;
        // No threat targets a new asset: an empty row, no heap.
        self.refs.asset_properties.push(Box::default());
        self.refs.assets.push(0);
        Ok(Op::RemoveAsset { id })
    }

    fn commit_rename_asset(
        &mut self,
        id: AssetId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let a = self.asset_mut(id)?;
        let name = std::mem::replace(&mut a.name, name);
        let provenance = std::mem::replace(&mut a.provenance, provenance);
        Ok(Op::RenameAsset {
            id,
            name,
            provenance,
        })
    }

    fn commit_set_asset_properties(
        &mut self,
        id: AssetId,
        properties: PropertySet,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let a = self.asset_mut(id)?;
        let properties = std::mem::replace(&mut a.properties, properties);
        let provenance = std::mem::replace(&mut a.provenance, provenance);
        Ok(Op::SetAssetProperties {
            id,
            properties,
            provenance,
        })
    }

    fn commit_remove_asset(&mut self, id: AssetId) -> Result<Op, ModelError> {
        let idx = self.asset_index(id)?;
        let a = self
            .assets
            .remove(idx)
            .ok_or(ModelError::UnknownAsset(id))?;
        Ok(Op::AddAsset {
            id,
            name: a.name,
            properties: a.properties,
            provenance: a.provenance,
        })
    }

    fn commit_rename_damage(
        &mut self,
        id: DamageId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let d = self.damage_mut(id)?;
        let name = std::mem::replace(&mut d.name, name);
        let provenance = std::mem::replace(&mut d.provenance, provenance);
        Ok(Op::RenameDamageScenario {
            id,
            name,
            provenance,
        })
    }

    fn commit_rename_threat(
        &mut self,
        id: ThreatId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let t = self.threat_mut(id)?;
        let name = std::mem::replace(&mut t.name, name);
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        Ok(Op::RenameThreatScenario {
            id,
            name,
            provenance,
        })
    }

    fn commit_add_damage(
        &mut self,
        id: DamageId,
        name: Arc<str>,
        impacts: &[(ImpactCategory, Level)],
        provenance: Provenance,
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
            provenance,
        };
        self.damages
            .push(id, d, self.slot_limit)
            .ok_or(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::DamageScenario,
            })?;
        self.refs.damages.push(0);
        Ok(Op::RemoveDamageScenario { id })
    }

    fn commit_set_impact(
        &mut self,
        id: DamageId,
        category: ImpactCategory,
        level: Option<Level>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let d = self.damage_mut(id)?;
        let slot = d
            .impacts
            .get_mut(category.index())
            .ok_or(ModelError::UnknownImpactCategory(category))?;
        let level = std::mem::replace(slot, level);
        let provenance = std::mem::replace(&mut d.provenance, provenance);
        Ok(Op::SetImpact {
            id,
            category,
            level,
            provenance,
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
            provenance: d.provenance,
        })
    }

    fn commit_catalog_ref(
        &mut self,
        threat: ThreatId,
        catalog_ref: CatalogRef,
        provenance: Provenance,
        add: bool,
    ) -> Result<Op, ModelError> {
        let t = self.threat_mut(threat)?;
        match (add, t.catalog_refs.binary_search(&catalog_ref)) {
            (true, Err(pos)) => {
                t.catalog_refs.insert(pos, catalog_ref);
                let provenance = std::mem::replace(&mut t.provenance, provenance);
                Ok(Op::RemoveCatalogRef {
                    threat,
                    catalog_ref,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                t.catalog_refs.remove(pos);
                let provenance = std::mem::replace(&mut t.provenance, provenance);
                Ok(Op::AddCatalogRef {
                    threat,
                    catalog_ref,
                    provenance,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateCatalogRef {
                threat,
                catalog_ref,
            }),
            (false, Err(_)) => Err(ModelError::CatalogRefNotFound {
                threat,
                catalog_ref,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_add_threat(
        &mut self,
        id: ThreatId,
        name: Arc<str>,
        asset: AssetId,
        category: ThreatCategory,
        mut catalog_refs: Vec<CatalogRef>,
        mut damages: Vec<DamageId>,
        provenance: Provenance,
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
            direct_feasibility: None,
            attack_tree: None,
            treatment: None,
            provenance,
        };
        let violates = self.methodology.violates(category);
        let idx = self.threats.push(id, t, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::ThreatScenario,
            },
        )?;
        if let Some(t) = self.threats.get(idx) {
            self.refs.threat(t, violates, true);
        }
        Ok(Op::RemoveThreatScenario { id })
    }

    fn commit_set_category(
        &mut self,
        id: ThreatId,
        category: ThreatCategory,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(id)?;
        let new = self.methodology.violates(category);
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        let old_category = std::mem::replace(&mut t.category, category);
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        let asset = t.asset;
        let old = self.methodology.violates(old_category);
        // New in before old out: a property both categories violate never
        // drops to 0, so it causes no reallocation.
        self.refs.properties(asset, new, true);
        self.refs.properties(asset, old, false);
        Ok(Op::SetThreatCategory {
            id,
            category: old_category,
            provenance,
        })
    }

    fn commit_link(
        &mut self,
        threat: ThreatId,
        damage: DamageId,
        provenance: Provenance,
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
                let provenance = std::mem::replace(&mut t.provenance, provenance);
                bump(&mut self.refs.damages, Some(d.index()), true);
                Ok(Op::UnlinkDamage {
                    threat,
                    damage,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                t.damages.remove(pos);
                let provenance = std::mem::replace(&mut t.provenance, provenance);
                bump(&mut self.refs.damages, Some(d.index()), false);
                Ok(Op::LinkDamage {
                    threat,
                    damage,
                    provenance,
                })
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
        self.refs.threat(&t, violates, false);
        Ok(Op::AddThreatScenario {
            id,
            name: t.name,
            asset,
            category: t.category,
            catalog_refs: t.catalog_refs,
            damages,
            provenance: t.provenance,
        })
    }

    fn commit_add_tree(
        &mut self,
        id: TreeId,
        tree: AttackTree,
        step_feasibility: Vec<(NodeId, FeasibilityLevel)>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let mut slots = vec![None; tree.len()];
        for (node, level) in step_feasibility {
            let node_idx = tree
                .idx_of(&node)
                .ok_or(ModelError::UnknownTreeNode { tree: id, node })?;
            let slot = slots
                .get_mut(node_idx.index())
                .ok_or(ModelError::UnknownTreeNode { tree: id, node })?;
            *slot = Some(level);
        }
        let step_mitigations = vec![Box::default(); tree.len()];
        let entry = TreeEntry {
            tree,
            step_feasibility: slots,
            step_mitigations,
            provenance,
        };
        self.trees
            .push(id, entry, self.slot_limit)
            .ok_or(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::AttackTree,
            })?;
        self.refs.trees.push(0);
        Ok(Op::RemoveAttackTree { id })
    }

    fn commit_set_step_feasibility(
        &mut self,
        tree: TreeId,
        node: NodeId,
        level: Option<FeasibilityLevel>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let entry = self.tree_mut(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let slot = entry
            .step_feasibility
            .get_mut(node_idx.index())
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let level = std::mem::replace(slot, level);
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::SetStepFeasibility {
            tree,
            node,
            level,
            provenance,
        })
    }

    fn commit_set_threat_feasibility(
        &mut self,
        threat: ThreatId,
        level: Option<FeasibilityLevel>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let t = self.threat_mut(threat)?;
        let level = std::mem::replace(&mut t.direct_feasibility, level);
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        Ok(Op::SetThreatFeasibility {
            threat,
            level,
            provenance,
        })
    }

    fn commit_set_threat_attack_tree(
        &mut self,
        threat: ThreatId,
        tree: Option<TreeId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(threat)?;
        let new_idx = tree.map(|t| self.tree_index(t)).transpose()?;
        let old_idx = self
            .threats
            .get(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?
            .attack_tree;
        let old_tree = old_idx
            .map(|i| {
                self.trees
                    .id(i)
                    .ok_or(ModelError::UnknownThreatScenario(threat))
            })
            .transpose()?;
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?;
        t.attack_tree = new_idx;
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        if let Some(old) = old_idx {
            bump(&mut self.refs.trees, Some(old.index()), false);
        }
        if let Some(new) = new_idx {
            bump(&mut self.refs.trees, Some(new.index()), true);
        }
        Ok(Op::SetThreatAttackTree {
            threat,
            tree: old_tree,
            provenance,
        })
    }

    fn commit_remove_tree(&mut self, id: TreeId) -> Result<Op, ModelError> {
        let idx = self.tree_index(id)?;
        let entry = self
            .trees
            .remove(idx)
            .ok_or(ModelError::UnknownAttackTree(id))?;
        let step_feasibility = entry
            .tree
            .iter()
            .zip(&entry.step_feasibility)
            .filter_map(|((_, node_id, _), &level)| level.map(|l| (node_id, l)))
            .collect();
        Ok(Op::AddAttackTree {
            id,
            tree: entry.tree,
            step_feasibility,
            provenance: entry.provenance,
        })
    }

    fn commit_set_risk_treatment(
        &mut self,
        threat: ThreatId,
        treatment: Option<RiskTreatment>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let t = self.threat_mut(threat)?;
        let treatment = std::mem::replace(&mut t.treatment, treatment);
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        Ok(Op::SetRiskTreatment {
            threat,
            treatment,
            provenance,
        })
    }

    fn commit_add_control(
        &mut self,
        id: ControlId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let control = SecurityControl { name, provenance };
        self.controls.push(id, control, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::SecurityControl,
            },
        )?;
        self.refs.controls.push(0);
        Ok(Op::RemoveSecurityControl { id })
    }

    fn commit_rename_control(
        &mut self,
        id: ControlId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let c = self.control_mut(id)?;
        let name = std::mem::replace(&mut c.name, name);
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        Ok(Op::RenameSecurityControl {
            id,
            name,
            provenance,
        })
    }

    fn commit_remove_control(&mut self, id: ControlId) -> Result<Op, ModelError> {
        let idx = self.control_index(id)?;
        let c = self
            .controls
            .remove(idx)
            .ok_or(ModelError::UnknownSecurityControl(id))?;
        Ok(Op::AddSecurityControl {
            id,
            name: c.name,
            provenance: c.provenance,
        })
    }

    fn commit_set_step_mitigation(
        &mut self,
        tree: TreeId,
        node: NodeId,
        control: ControlId,
        feasibility: Option<FeasibilityLevel>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let tree_idx = self.tree_index(tree)?;
        let ctrl_idx = self.control_index(control)?;
        let (node_idx, found) = {
            let entry = self
                .trees
                .get(tree_idx)
                .ok_or(ModelError::UnknownAttackTree(tree))?;
            let node_idx = entry
                .tree
                .idx_of(&node)
                .ok_or(ModelError::UnknownTreeNode { tree, node })?;
            let row = entry
                .step_mitigations
                .get(node_idx.index())
                .ok_or(ModelError::UnknownTreeNode { tree, node })?;
            (node_idx, self.find_mitigation(row, control))
        };
        let entry = self
            .trees
            .get_mut(tree_idx)
            .ok_or(ModelError::UnknownAttackTree(tree))?;
        let slot = entry
            .step_mitigations
            .get_mut(node_idx.index())
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let mut row = std::mem::take(slot).into_vec();
        let old = match (feasibility, found) {
            (Some(new_f), Ok(pos)) => {
                let old_f = std::mem::replace(&mut row[pos].feasibility, new_f);
                Some(old_f)
            }
            (Some(new_f), Err(pos)) => {
                row.insert(
                    pos,
                    StepMitigation {
                        control: ctrl_idx,
                        feasibility: new_f,
                    },
                );
                bump(&mut self.refs.controls, Some(ctrl_idx.index()), true);
                None
            }
            (None, Ok(pos)) => {
                let removed = row.remove(pos);
                bump(&mut self.refs.controls, Some(ctrl_idx.index()), false);
                Some(removed.feasibility)
            }
            (None, Err(_)) => None,
        };
        *slot = row.into_boxed_slice();
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::SetStepMitigation {
            tree,
            node,
            control,
            feasibility: old,
            provenance,
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
            && self.trees.len() == other.trees.len()
            && self.controls.len() == other.controls.len()
            && self
                .assets()
                .zip(other.assets())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .damage_scenarios()
                .zip(other.damage_scenarios())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .security_controls()
                .zip(other.security_controls())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .trees
                .iter()
                .zip(other.trees.iter())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_tree(x, other, y))
            && self
                .threat_scenarios()
                .zip(other.threat_scenarios())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_threat(x, other, y))
    }
}

impl Eq for Project {}

impl Project {
    /// Compares two trees from two projects by content, with control
    /// references resolved to public IDs. O(V + M).
    fn same_tree(&self, x: &TreeEntry, other: &Project, y: &TreeEntry) -> bool {
        x.tree == y.tree
            && x.step_feasibility == y.step_feasibility
            && x.provenance == y.provenance
            && x.step_mitigations.len() == y.step_mitigations.len()
            && x.step_mitigations
                .iter()
                .zip(&y.step_mitigations)
                .all(|(mx, my)| {
                    mx.len() == my.len()
                        && mx.iter().zip(my.iter()).all(|(a, b)| {
                            self.control_key(a.control) == other.control_key(b.control)
                                && a.feasibility == b.feasibility
                        })
                })
    }

    /// Compares two threats from two projects by content, with references
    /// resolved to public IDs. O(k).
    fn same_threat(&self, x: &ThreatScenario, other: &Project, y: &ThreatScenario) -> bool {
        x.name == y.name
            && x.category == y.category
            && x.catalog_refs == y.catalog_refs
            && x.direct_feasibility == y.direct_feasibility
            && x.treatment == y.treatment
            && x.provenance == y.provenance
            && self.assets.id(x.asset) == other.assets.id(y.asset)
            && self.tree_key(x.attack_tree) == other.tree_key(y.attack_tree)
            && x.damages.len() == y.damages.len()
            && x.damages
                .iter()
                .zip(&y.damages)
                .all(|(&a, &b)| self.damage_key(a) == other.damage_key(b))
    }

    /// Checks every structural invariant and returns a description of the
    /// first one broken. Time O(n log n + L + m).
    #[cfg(test)]
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        let m = &*self.methodology;
        arena_invariants("asset", &self.assets)?;
        arena_invariants("damage", &self.damages)?;
        arena_invariants("threat", &self.threats)?;
        arena_invariants("tree", &self.trees)?;
        arena_invariants("control", &self.controls)?;
        self.entity_invariants()?;
        let controls = self.tree_invariants()?;

        let mut assets = vec![0u32; self.assets.slot_count()];
        let mut damages = vec![0u32; self.damages.slot_count()];
        let mut trees = vec![0u32; self.trees.slot_count()];
        // Recounted independently of `count_row`: (asset slot, property)
        // -> threats, in ascending order.
        let mut pairs: BTreeMap<(usize, Property), u32> = BTreeMap::new();
        for (_, id, t) in self.threat_scenarios() {
            check_name(EntityKind::ThreatScenario, &t.name)
                .map_err(|e| format!("threat {id}: {e}"))?;
            check_provenance(&t.provenance).map_err(|e| format!("threat {id}: {e}"))?;
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
            if t.damages.len() > MAX_DAMAGE_LINKS {
                return Err(format!("threat {id} has too many damage links"));
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
            if let Some(level) = t.direct_feasibility {
                m.check_feasibility_level(level)
                    .map_err(|e| format!("threat {id}: {e}"))?;
            }
            if let Some(tree) = t.attack_tree {
                if self.trees.get(tree).is_none() {
                    return Err(format!("threat {id} links a dead tree slot"));
                }
                bump(&mut trees, Some(tree.index()), true);
            }
            bump(&mut assets, Some(t.asset.index()), true);
            for d in &t.damages {
                bump(&mut damages, Some(d.index()), true);
            }
            for p in m.violates(t.category).iter() {
                *pairs.entry((t.asset.index(), p)).or_insert(0) += 1;
            }
        }
        let mut rows = vec![Vec::new(); self.assets.slot_count()];
        for ((a, property), threats) in pairs {
            rows.get_mut(a)
                .ok_or(format!("a threat targets asset slot {a}, beyond the arena"))?
                .push(PropertyCount { property, threats });
        }
        let want = RefCounts {
            asset_properties: rows.into_iter().map(Vec::into_boxed_slice).collect(),
            assets,
            damages,
            trees,
            controls,
        };
        self.refs_invariants(&want)
    }

    #[cfg(test)]
    fn entity_invariants(&self) -> Result<(), String> {
        let m = &*self.methodology;
        for (_, id, a) in self.assets() {
            check_name(EntityKind::Asset, &a.name).map_err(|e| format!("asset {id}: {e}"))?;
            check_provenance(&a.provenance).map_err(|e| format!("asset {id}: {e}"))?;
            if a.properties.is_empty() {
                return Err(format!("asset {id} has no properties"));
            }
            m.check_properties(a.properties)
                .map_err(|e| format!("asset {id}: {e}"))?;
        }
        for (_, id, d) in self.damage_scenarios() {
            check_name(EntityKind::DamageScenario, &d.name)
                .map_err(|e| format!("damage {id}: {e}"))?;
            check_provenance(&d.provenance).map_err(|e| format!("damage {id}: {e}"))?;
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
        for (_, id, c) in self.security_controls() {
            check_name(EntityKind::SecurityControl, &c.name)
                .map_err(|e| format!("control {id}: {e}"))?;
            check_provenance(&c.provenance).map_err(|e| format!("control {id}: {e}"))?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn tree_invariants(&self) -> Result<Vec<u32>, String> {
        let m = &*self.methodology;
        let mut controls = vec![0u32; self.controls.slot_count()];
        for (_, id, entry) in self.trees.iter() {
            check_provenance(&entry.provenance).map_err(|e| format!("tree {id}: {e}"))?;
            if entry.step_feasibility.len() != entry.tree.len() {
                return Err(format!(
                    "tree {id} has {} feasibility slots",
                    entry.step_feasibility.len()
                ));
            }
            if entry.step_mitigations.len() != entry.tree.len() {
                return Err(format!(
                    "tree {id} has {} mitigation slots",
                    entry.step_mitigations.len()
                ));
            }
            entry
                .tree
                .topological_order()
                .map_err(|e| format!("tree {id}: {e}"))?;
            for (((_, node_id, node), &level), mitigations) in entry
                .tree
                .iter()
                .zip(&entry.step_feasibility)
                .zip(&entry.step_mitigations)
            {
                check_name(EntityKind::TreeNode, node.label())
                    .map_err(|e| format!("tree {id} node {node_id}: {e}"))?;
                match (node, level) {
                    (Node::Gate { .. }, Some(_)) => {
                        return Err(format!("tree {id} gate {node_id} has a feasibility rating"));
                    }
                    (Node::Step { .. }, Some(l)) => m
                        .check_feasibility_level(l)
                        .map_err(|e| format!("tree {id} node {node_id}: {e}"))?,
                    _ => {}
                }
                if matches!(node, Node::Gate { .. }) && !mitigations.is_empty() {
                    return Err(format!("tree {id} gate {node_id} has step mitigations"));
                }
                if mitigations.len() > MAX_STEP_MITIGATIONS {
                    return Err(format!("tree {id} node {node_id} has too many mitigations"));
                }
                let keys: Vec<_> = mitigations
                    .iter()
                    .map(|mt| self.control_key(mt.control))
                    .collect();
                if keys.iter().any(Option::is_none) {
                    return Err(format!(
                        "tree {id} node {node_id} references a dead control slot"
                    ));
                }
                if !keys.windows(2).all(|w| w[0] < w[1]) {
                    return Err(format!(
                        "tree {id} node {node_id} mitigations not sorted and unique"
                    ));
                }
                for mt in mitigations {
                    m.check_feasibility_level(mt.feasibility)
                        .map_err(|e| format!("tree {id} node {node_id}: {e}"))?;
                    bump(&mut controls, Some(mt.control.index()), true);
                }
            }
        }
        Ok(controls)
    }

    /// Checks that the reference counts are canonical and equal a recount
    /// `want`. O(asset slots + targeted pairs).
    #[cfg(test)]
    fn refs_invariants(&self, want: &RefCounts) -> Result<(), String> {
        let have = &self.refs;
        if have.asset_properties.len() != want.asset_properties.len() {
            return Err(format!(
                "{} property-count rows for {} asset slots",
                have.asset_properties.len(),
                want.asset_properties.len()
            ));
        }
        for (slot, (h, w)) in have
            .asset_properties
            .iter()
            .zip(&want.asset_properties)
            .enumerate()
        {
            if h.iter().any(|c| c.threats == 0) {
                return Err(format!("asset slot {slot} stores a zero count: {h:?}"));
            }
            if !h.windows(2).all(|w| w[0].property < w[1].property) {
                return Err(format!(
                    "asset slot {slot} row not sorted and unique: {h:?}"
                ));
            }
            if h != w {
                return Err(format!(
                    "asset slot {slot} property counts differ from a recount: {h:?} vs {w:?}"
                ));
            }
        }
        if have.assets != want.assets {
            return Err(format!(
                "asset counts differ from a recount: {:?} vs {:?}",
                have.assets, want.assets
            ));
        }
        if have.damages != want.damages {
            return Err(format!(
                "damage counts differ from a recount: {:?} vs {:?}",
                have.damages, want.damages
            ));
        }
        if have.trees != want.trees {
            return Err(format!(
                "tree counts differ from a recount: {:?} vs {:?}",
                have.trees, want.trees
            ));
        }
        if have.controls != want.controls {
            return Err(format!(
                "control counts differ from a recount: {:?} vs {:?}",
                have.controls, want.controls
            ));
        }
        // The derived `==` must agree with the field-by-field checks.
        if have != want {
            return Err("reference counts differ from a recount".to_owned());
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
        let kind = EntityKind::Asset;
        assert_eq!(check_name(kind, "a"), Ok(()));
        assert_eq!(check_name(kind, " a "), Ok(()));
        assert_eq!(check_name(kind, ""), Err(ModelError::EmptyName { kind }));
        assert_eq!(
            check_name(kind, " \t\n"),
            Err(ModelError::EmptyName { kind })
        );
        assert_eq!(check_name(kind, &"é".repeat(256)), Ok(()));
        assert_eq!(
            check_name(kind, &"é".repeat(257)),
            Err(ModelError::NameTooLong { kind, max: 256 })
        );
    }

    #[test]
    fn check_provenance_rules() {
        assert_eq!(check_provenance(&Provenance::default()), Ok(()));
        assert_eq!(check_provenance(&Provenance::import("", 42)), Ok(()));
        assert_eq!(check_provenance(&Provenance::human("alice", 1)), Ok(()));
        assert_eq!(check_provenance(&Provenance::human(" alice ", 1)), Ok(()));
        assert_eq!(
            check_provenance(&Provenance::human(" \t\n", 1)),
            Err(ModelError::EmptyActor)
        );
        assert_eq!(
            check_provenance(&Provenance::human("é".repeat(256), 1)),
            Ok(())
        );
        assert_eq!(
            check_provenance(&Provenance::human("é".repeat(257), 1)),
            Err(ModelError::ActorTooLong { max: 256 })
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
            provenance: Provenance::default(),
        })
        .expect("valid");
        assert_eq!(p.check_invariants(), Ok(()));
        p.refs.assets[0] = 3;
        assert!(p.check_invariants().is_err());
    }

    fn pc(property: Property, threats: u32) -> PropertyCount {
        PropertyCount { property, threats }
    }

    /// A row with every property of `set` counted `threats` times.
    fn row_of(set: PropertySet, threats: u32) -> Vec<PropertyCount> {
        set.iter().map(|p| pc(p, threats)).collect()
    }

    /// The pair of threat categories with different `violates` that share
    /// the most properties (ties: the first found).
    fn category_pair(m: &Methodology) -> (ThreatCategory, ThreatCategory) {
        let cats: Vec<_> = m.threat_categories().collect();
        let mut best = None;
        let mut best_shared = 0;
        for &a in &cats {
            for &b in &cats {
                let (va, vb) = (m.violates(a), m.violates(b));
                if va == vb {
                    continue;
                }
                let shared = va.iter().filter(|&p| vb.contains(p)).count();
                if best.is_none() || shared > best_shared {
                    best = Some((a, b));
                    best_shared = shared;
                }
            }
        }
        best.expect("two categories with different violates")
    }

    fn threat_op(n: u64, asset: AssetId, category: ThreatCategory) -> Op {
        Op::AddThreatScenario {
            id: testkit::threat_id(n),
            name: "t".into(),
            asset,
            category,
            catalog_refs: vec![],
            damages: vec![],
            provenance: Provenance::default(),
        }
    }

    fn assert_sparse(m: &Arc<Methodology>) {
        let mut p = Project::new(Arc::clone(m));
        let ok = |p: &Project| assert_eq!(p.check_invariants(), Ok(()));
        // (a) 1,000 assets carrying every property, no threats: no counts.
        for n in 0..1_000 {
            p.apply(Op::AddAsset {
                id: testkit::asset_id(n),
                name: "a".into(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            })
            .expect("valid asset");
        }
        assert_eq!(p.refs.asset_properties.len(), 1_000);
        assert!(p.refs.asset_properties.iter().all(|r| r.is_empty()));
        ok(&p);

        let a = testkit::asset_id(0);
        let a_idx = p.asset_idx(&a).expect("live");
        let (from, to) = category_pair(m);
        let (vf, vt) = (m.violates(from), m.violates(to));
        let others_empty = |p: &Project| {
            p.refs
                .asset_properties
                .iter()
                .enumerate()
                .all(|(i, r)| i == a_idx.index() || r.is_empty())
        };
        let row = |p: &Project| p.refs.asset_properties[a_idx.index()].to_vec();

        // (b) one threat: the row is exactly `violates`, sorted, counts 1.
        p.apply(threat_op(1, a, from)).expect("valid threat");
        assert_eq!(row(&p), row_of(vf, 1));
        assert!(row(&p).windows(2).all(|w| w[0].property < w[1].property));
        assert!(others_empty(&p));
        ok(&p);

        // (c) a second threat of the same category: counts 2.
        p.apply(threat_op(2, a, from)).expect("valid threat");
        assert_eq!(row(&p), row_of(vf, 2));
        assert!(others_empty(&p));
        ok(&p);

        // (d) recategorize the second threat: entries move; shared
        // properties keep their counts.
        let undo = p
            .apply(Op::SetThreatCategory {
                id: testkit::threat_id(2),
                category: to,
                provenance: Provenance::default(),
            })
            .expect("valid recategorization");
        let want: Vec<_> = vf
            .iter()
            .chain(vt.iter())
            .collect::<PropertySet>()
            .iter()
            .map(|q| pc(q, u32::from(vf.contains(q)) + u32::from(vt.contains(q))))
            .collect();
        assert_eq!(row(&p), want);
        for q in vf.iter().filter(|&q| vt.contains(q)) {
            assert_eq!(p.asset_property_refs(a_idx, q), 2, "shared {q:?}");
        }
        for q in vt.iter().filter(|&q| !vf.contains(q)) {
            assert_eq!(p.asset_property_refs(a_idx, q), 1, "moved in {q:?}");
        }
        for q in vf.iter().filter(|&q| !vt.contains(q)) {
            assert_eq!(p.asset_property_refs(a_idx, q), 1, "moved out {q:?}");
        }
        assert!(others_empty(&p));
        ok(&p);
        p.apply(undo).expect("undo");
        assert_eq!(row(&p), row_of(vf, 2));
        p.apply(Op::SetThreatCategory {
            id: testkit::threat_id(2),
            category: to,
            provenance: Provenance::default(),
        })
        .expect("redo");
        ok(&p);

        // (e) removing every threat empties the row again.
        for n in [1, 2] {
            p.apply(Op::RemoveThreatScenario {
                id: testkit::threat_id(n),
            })
            .expect("valid removal");
            ok(&p);
        }
        assert!(p.refs.asset_properties.iter().all(|r| r.is_empty()));

        // (f) removing the asset leaves an empty row on its tombstone.
        p.apply(Op::RemoveAsset { id: a }).expect("valid removal");
        assert_eq!(p.asset(a_idx), None);
        assert_eq!(p.refs.asset_properties.len(), 1_000);
        assert!(p.refs.asset_properties[a_idx.index()].is_empty());
        ok(&p);
    }

    #[test]
    fn refcounts_are_sparse() {
        assert_sparse(&testkit::builtin());
        assert_sparse(&testkit::second());
    }

    #[test]
    fn refcounts_category_pair_shares_a_property_in_second() {
        // Guards (d) above: in the second methodology the chosen pair
        // shares a property, so the "shared keeps its count" path runs.
        let m = testkit::second();
        let (from, to) = category_pair(&m);
        assert!(m.violates(from).iter().any(|q| m.violates(to).contains(q)));
    }

    #[test]
    fn count_row_keeps_canonical_form() {
        let q = |i: u8| Property::new(i);
        let set = |is: &[u8]| is.iter().map(|&i| q(i)).collect::<PropertySet>();
        let mut row: Box<[PropertyCount]> = Box::default();
        count_row(&mut row, set(&[5, 1]), true);
        assert_eq!(&*row, [pc(q(1), 1), pc(q(5), 1)]);
        // In place: every property present.
        count_row(&mut row, set(&[5]), true);
        assert_eq!(&*row, [pc(q(1), 1), pc(q(5), 2)]);
        // Merge: new properties before, between, and after.
        count_row(&mut row, set(&[0, 3, 5, 9]), true);
        assert_eq!(
            &*row,
            [
                pc(q(0), 1),
                pc(q(1), 1),
                pc(q(3), 1),
                pc(q(5), 3),
                pc(q(9), 1)
            ]
        );
        assert_eq!(row.len(), 5);
        // Out, nothing drops to 0: in place.
        count_row(&mut row, set(&[5]), false);
        assert_eq!(row_count(&row, q(5)), 2);
        // Out, some drop to 0: removed; a missing property is ignored.
        count_row(&mut row, set(&[0, 5, 9, 42]), false);
        assert_eq!(&*row, [pc(q(1), 1), pc(q(3), 1), pc(q(5), 1)]);
        count_row(&mut row, set(&[1, 3, 5]), false);
        assert!(row.is_empty());
        // Out of an empty row: still empty.
        count_row(&mut row, set(&[2]), false);
        assert!(row.is_empty());
    }

    #[test]
    fn row_count_looks_up_by_property() {
        let q = |i: u8| Property::new(i);
        let row = [pc(q(2), 4), pc(q(7), 1)];
        assert_eq!(row_count(&row, q(2)), 4);
        assert_eq!(row_count(&row, q(7)), 1);
        assert_eq!(row_count(&row, q(3)), 0);
        assert_eq!(row_count(&[], q(0)), 0);
    }

    #[test]
    fn refcounts_ignore_missing_asset_slots() {
        let mut refs = RefCounts::default();
        let p = Property::new(0);
        refs.properties(AssetIdx::from_u32(3), PropertySet::empty().with(p), true);
        assert_eq!(refs, RefCounts::default());
        assert_eq!(refs.property(AssetIdx::from_u32(3), p), 0);
    }

    #[test]
    fn check_invariants_detects_bad_property_counts() {
        let m = testkit::builtin();
        let mut p = Project::new(Arc::clone(&m));
        let a = testkit::asset_id(1);
        p.apply(Op::AddAsset {
            id: a,
            name: "a".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        })
        .expect("valid");
        // Two threats over two different properties, so the row has two
        // entries to put out of order.
        let cats: Vec<_> = m.threat_categories().collect();
        let (c0, c1) = (cats[0], cats[1]);
        assert_ne!(m.violates(c0), m.violates(c1));
        p.apply(threat_op(1, a, c0)).expect("valid");
        p.apply(threat_op(2, a, c1)).expect("valid");
        assert_eq!(p.check_invariants(), Ok(()));
        let good = p.refs.asset_properties[0].clone();
        assert_eq!(good.len(), 2);

        // A wrong count.
        let mut bad = good.to_vec();
        bad[0].threats = 2;
        p.refs.asset_properties[0] = bad.into_boxed_slice();
        assert!(p.check_invariants().is_err());

        // An entry with `threats: 0`.
        let mut bad = good.to_vec();
        bad.push(pc(Property::new(5), 0));
        bad.sort_by_key(|c| c.property);
        p.refs.asset_properties[0] = bad.into_boxed_slice();
        assert!(p.check_invariants().is_err());

        // Two entries out of order.
        let mut bad = good.to_vec();
        bad.reverse();
        p.refs.asset_properties[0] = bad.into_boxed_slice();
        assert!(p.check_invariants().is_err());

        p.refs.asset_properties[0] = good;
        assert_eq!(p.check_invariants(), Ok(()));
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
