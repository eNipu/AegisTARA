//! The project: assets, damage scenarios, threat scenarios, and attack
//! trees over one methodology, changed only through typed [`Op`]s.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::asset::{Asset, PropertySet};
use crate::control::{
    CeilingSource, ControlRule, CybersecurityClaim, CybersecurityGoal, DecidingStep,
    MAX_CONTROL_RULES, MAX_CONTROL_TARGETS, MAX_GOAL_CONTROLS, MAX_STEP_MITIGATIONS, RiskTreatment,
    SecurityControl, StepCeiling, StepMitigation,
};
use crate::damage::DamageScenario;
use crate::description::{
    Described, ElementRef, MAX_DESCRIPTION_CHARS, MAX_DESCRIPTION_TOTAL_BYTES,
    MAX_DESCRIPTION_TRANSLATIONS, Translation, check_lang, check_text,
};
use crate::diff::{EntityDiff, ProjectDiff, RiskDelta, RiskSnapshot, ThreatRiskDelta};
use crate::element::ElementKind;
use crate::error::{EntityKind, ModelError};
use crate::eval::{Evaluation, MaxMin, TreeEvalCache, evaluate};
use crate::id::{
    ArenaIndex, AssetId, AssetIdx, ClaimId, ClaimIdx, ComponentId, ComponentIdx, ControlId,
    ControlIdx, DamageId, DamageIdx, DataFlowId, DataFlowIdx, ElementId, GoalId, GoalIdx,
    InterfaceId, InterfaceIdx, NodeId, NodeIdx, PartyId, PartyIdx, ThreatId, ThreatIdx, TreeId,
    TreeIdx, TrustBoundaryId, TrustBoundaryIdx, VisualPrefix,
};
use crate::item::{
    Component, DataFlow, Interface, ItemIdx, ItemRef, MAX_COMPONENT_DEPTH,
    MAX_TRUST_BOUNDARY_MEMBERS, TrustBoundary,
};
use crate::lint::{LintCode, LintFinding};
use crate::methodology::{
    AttackPotential, AttackType, CatalogRef, ControlKind, FactorLevel, FeasibilityFactor,
    FeasibilityLevel, ImpactCategory, InterfaceKind, Level, Methodology, Property, RiskLevel,
    ThreatCategory,
};
use crate::op::Op;
use crate::party::{MAX_PARTIES, MAX_RESPONSIBLE, Party, PartyKind};
use crate::paths::{AttackPath, ChokePoint};
use crate::provenance::Provenance;
use crate::sensitivity::{
    FeasibilityTippingPoint, ImpactTippingPoint, StepTippingPoint, ThreatSensitivity,
};
use crate::threat::{ImpactRisk, MAX_CATALOG_REFS, MAX_DAMAGE_LINKS, ThreatRisk, ThreatScenario};
use crate::tree::{AttackTree, MAX_ATTACK_TREE_NODES, MAX_TREE_OP_NODES, Node};

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
    visuals: Vec<u32>,
    next_visual: u32,
    removed_visuals: BTreeMap<Id, u32>,
    by_id: BTreeMap<Id, Idx>,
}

impl<Id: Copy + Ord, Idx: ArenaIndex, T> Arena<Id, Idx, T> {
    fn new() -> Self {
        Arena {
            slots: Vec::new(),
            ids: Vec::new(),
            visuals: Vec::new(),
            next_visual: 0,
            removed_visuals: BTreeMap::new(),
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

    /// The 0-based visual sequence number of a live entity. O(1).
    fn visual_num(&self, idx: Idx) -> Option<u32> {
        self.get(idx)?;
        self.visuals.get(idx.position()).copied()
    }

    /// Overrides the visual sequence number of a loaded entity and updates
    /// `next_visual`. O(log n).
    fn set_loaded_visual(&mut self, id: &Id, vis: u32, next_visual: u32) {
        if let Some(idx) = self.idx(id)
            && let Some(slot) = self.visuals.get_mut(idx.position())
        {
            *slot = vis;
            self.next_visual = next_visual;
        }
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
        let vis = self.removed_visuals.remove(&id).unwrap_or_else(|| {
            let current = self.next_visual;
            self.next_visual = self.next_visual.saturating_add(1);
            current
        });
        self.next_visual = self.next_visual.max(vis.saturating_add(1));
        self.slots.push(Some(value));
        self.ids.push(id);
        self.visuals.push(vis);
        self.by_id.insert(id, idx);
        Some(idx)
    }

    /// Tombstones a live entity and returns it. O(log n).
    fn remove(&mut self, idx: Idx) -> Option<T> {
        let id = self.id(idx)?;
        let vis = self.visuals.get(idx.position()).copied()?;
        let value = self.slots.get_mut(idx.position())?.take();
        self.by_id.remove(&id);
        self.removed_visuals.insert(id, vis);
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
/// feasibility ratings, per-node attack potential factor ratings, per-node
/// step mitigations, and provenance. Gate slots are always `None` and empty.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TreeEntry {
    tree: AttackTree,
    step_feasibility: Vec<Option<FeasibilityLevel>>,
    step_factors: Vec<Box<[Option<FactorLevel>]>>,
    step_mitigations: Vec<Box<[StepMitigation]>>,
    /// What the steps go through and their attack types (CFG-01, CFG-02).
    /// `None` until some step has either, so a tree that uses neither costs
    /// 8 bytes. Never holds only `None`s (see [`StepLinks`]).
    step_links: Option<Box<StepLinks>>,
    provenance: Provenance,
}

/// Per node: the component or interface a step goes through, and its
/// pack-defined attack type. Both vectors have one slot per node, and at
/// least one slot is `Some` while the value exists, so equal trees compare
/// equal however they got there.
#[derive(Clone, Debug, PartialEq, Eq)]
struct StepLinks {
    via: Vec<Option<ItemIdx>>,
    attack_type: Vec<Option<AttackType>>,
}

impl StepLinks {
    fn is_empty(&self) -> bool {
        self.via.iter().all(Option::is_none) && self.attack_type.iter().all(Option::is_none)
    }
}

impl TreeEntry {
    fn via_at(&self, i: usize) -> Option<ItemIdx> {
        self.step_links.as_ref()?.via.get(i).copied().flatten()
    }

    fn attack_type_at(&self, i: usize) -> Option<AttackType> {
        self.step_links
            .as_ref()?
            .attack_type
            .get(i)
            .copied()
            .flatten()
    }

    /// The slots that have a "through" item, with the item.
    fn vias(&self) -> impl Iterator<Item = (usize, ItemIdx)> + '_ {
        self.step_links
            .iter()
            .flat_map(|l| l.via.iter().enumerate())
            .filter_map(|(i, v)| Some((i, (*v)?)))
    }

    /// How many steps have a "through" item or an attack type.
    fn linked_steps(&self) -> usize {
        self.step_links.as_ref().map_or(0, |l| {
            l.via
                .iter()
                .zip(&l.attack_type)
                .filter(|(v, t)| v.is_some() || t.is_some())
                .count()
        })
    }

    /// Sets one slot of a link vector and returns the old value, keeping
    /// `step_links` canonical. `None` over `None` allocates nothing.
    fn set_link<T: Copy>(
        &mut self,
        i: usize,
        value: Option<T>,
        field: fn(&mut StepLinks) -> &mut Vec<Option<T>>,
    ) -> Option<T> {
        if self.step_links.is_none() && value.is_none() {
            return None;
        }
        let len = self.tree.len();
        let links = self.step_links.get_or_insert_with(|| {
            Box::new(StepLinks {
                via: vec![None; len],
                attack_type: vec![None; len],
            })
        });
        let old = field(links)
            .get_mut(i)
            .and_then(|slot| std::mem::replace(slot, value));
        if links.is_empty() {
            self.step_links = None;
        }
        old
    }

    fn insert_links(&mut self, i: usize) {
        if let Some(l) = self.step_links.as_mut() {
            l.via.insert(i, None);
            l.attack_type.insert(i, None);
        }
    }

    fn remove_links(&mut self, i: usize) {
        if let Some(l) = self.step_links.as_mut() {
            l.via.remove(i);
            l.attack_type.remove(i);
        }
    }

    fn step_attack_potential(&self, m: &Methodology, idx: NodeIdx) -> Option<AttackPotential> {
        let row = self.step_factors.get(idx.index())?;
        if row.is_empty() {
            return None;
        }
        m.resolve_attack_potential(row)
    }

    fn effective_step_feasibility(
        &self,
        m: &Methodology,
        idx: NodeIdx,
    ) -> Option<FeasibilityLevel> {
        let row = self
            .step_factors
            .get(idx.index())
            .map_or(&[][..], Box::as_ref);
        m.resolve_attack_potential_feasibility(row)
            .or_else(|| self.step_feasibility.get(idx.index()).copied().flatten())
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
    /// Per security control slot: how many live cybersecurity goals link
    /// to it.
    control_goals: Vec<u32>,
    /// Per cybersecurity goal slot: how many live threats reference it.
    goals: Vec<u32>,
    /// Per cybersecurity claim slot: how many live threats reference it.
    claims: Vec<u32>,
    /// Per component slot: how many live child components reference it as `parent`.
    component_children: Vec<u32>,
    /// Per component slot: how many live interfaces belong to it.
    component_interfaces: Vec<u32>,
    /// Per component slot: how many live data flows reference it as `from` or `to`.
    component_flows: Vec<u32>,
    /// Per component slot: how many live trust boundaries contain it.
    component_boundaries: Vec<u32>,
    /// Per component slot: how many live assets are bound to it.
    component_assets: Vec<u32>,
    /// Per component slot: how many live security controls are applied to it.
    component_controls: Vec<u32>,
    /// Per interface slot: how many live data flows reference it.
    interfaces: Vec<u32>,
    /// Per interface slot: how many live security controls are applied to it.
    interface_controls: Vec<u32>,
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

fn adjacent_feasibility(cur: FeasibilityLevel, count: usize) -> [Option<FeasibilityLevel>; 2] {
    let idx = cur.index();
    [
        idx.checked_sub(1)
            .and_then(|prev| u8::try_from(prev).ok())
            .map(FeasibilityLevel::new),
        idx.checked_add(1)
            .filter(|&next| next < count)
            .and_then(|next| u8::try_from(next).ok())
            .map(FeasibilityLevel::new),
    ]
}

fn adjacent_impact(cur: Level, count: usize) -> [Option<Level>; 2] {
    let idx = cur.index();
    [
        idx.checked_sub(1)
            .and_then(|prev| u8::try_from(prev).ok())
            .map(Level::new),
        idx.checked_add(1)
            .filter(|&next| next < count)
            .and_then(|next| u8::try_from(next).ok())
            .map(Level::new),
    ]
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
        if let Some(goal) = t.goal {
            bump(&mut self.goals, Some(goal.index()), up);
        }
        if let Some(claim) = t.claim {
            bump(&mut self.claims, Some(claim.index()), up);
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
    goals: Arena<GoalId, GoalIdx, CybersecurityGoal>,
    claims: Arena<ClaimId, ClaimIdx, CybersecurityClaim>,
    parties: Arena<PartyId, PartyIdx, Party>,
    components: Arena<ComponentId, ComponentIdx, Component>,
    interfaces: Arena<InterfaceId, InterfaceIdx, Interface>,
    data_flows: Arena<DataFlowId, DataFlowIdx, DataFlow>,
    trust_boundaries: Arena<TrustBoundaryId, TrustBoundaryIdx, TrustBoundary>,
    /// Derived data, rebuilt from `threats`, `trees`, and `goals` (D10).
    refs: RefCounts,
    /// Descriptions and the project's notes, in ID order (job 064).
    descriptions: BTreeMap<ElementRef, Described>,
    /// Derived: the bytes of all description texts together (D10).
    description_bytes: usize,
    /// Steps whose rating came from a library item and awaits review
    /// (job 057), in ID order.
    review_marks: std::collections::BTreeSet<(TreeId, NodeId)>,
    /// The languages descriptions are translated into, and who set them.
    languages: (Arc<[Arc<str>]>, Provenance),
    /// Elements of the generic kinds (job 205).
    pub(crate) elements: crate::element::ElementStore,
}

/// Adds `ids` to the `modified` list of `diff`, except the ones that were
/// added or removed (those are already listed), keeping the list sorted.
fn merge_modified<I: Copy + Ord>(diff: &mut EntityDiff<I>, ids: Vec<I>) {
    let mut modified = diff.modified.to_vec();
    modified.extend(ids.into_iter().filter(|id| {
        diff.added.binary_search(id).is_err() && diff.removed.binary_search(id).is_err()
    }));
    modified.sort_unstable();
    modified.dedup();
    diff.modified = modified.into_boxed_slice();
}

/// Checks an entity name. Time O(name length).
pub(crate) fn check_name(kind: EntityKind, name: &str) -> Result<(), ModelError> {
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

/// Checks a party name without putting the name in the error text.
/// Time O(name length).
fn check_party_name(name: &str) -> Result<(), ModelError> {
    if name.trim().is_empty() {
        return Err(ModelError::PartyNameEmpty);
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ModelError::PartyNameTooLong {
            max: MAX_NAME_CHARS,
        });
    }
    Ok(())
}

/// Checks operation provenance. Time O(actor length).
pub(crate) fn check_provenance(provenance: &Provenance) -> Result<(), ModelError> {
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
            goals: Arena::new(),
            claims: Arena::new(),
            parties: Arena::new(),
            components: Arena::new(),
            interfaces: Arena::new(),
            data_flows: Arena::new(),
            trust_boundaries: Arena::new(),
            refs: RefCounts::default(),
            descriptions: BTreeMap::new(),
            description_bytes: 0,
            review_marks: std::collections::BTreeSet::new(),
            languages: (Arc::from([]), Provenance::default()),
            elements: crate::element::ElementStore::default(),
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

    /// Returns the rated [`FactorLevel`] for `factor` on `node` in `tree` in
    /// O(1), or `None` if `tree`, `node`, or `factor` is out of range, `node`
    /// is a gate, or that factor is unrated on `node`. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Extract key"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// let eq = m.feasibility_factor("equipment").ok_or("no factor")?;
    /// let bespoke = m.factor_level(eq, "bespoke").ok_or("no level")?;
    /// project.apply(Op::SetStepFactor {
    ///     tree: tree_id, node: step, factor: eq, level: Some(bespoke),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// assert_eq!(project.step_factor(idx, root, eq), Some(bespoke));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_factor(
        &self,
        tree: TreeIdx,
        node: NodeIdx,
        factor: FeasibilityFactor,
    ) -> Option<FactorLevel> {
        self.trees
            .get(tree)?
            .step_factors
            .get(node.index())?
            .get(factor.index())
            .copied()
            .flatten()
    }

    /// Iterates the rated `(FeasibilityFactor, FactorLevel)` pairs on `node` in
    /// `tree` in methodology declaration order. Empty if `tree` or `node` is
    /// out of range, `node` is a gate, or no factors are rated on `node`. Time
    /// O(F) for the whole walk, F <= 16; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Extract key"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// let eq = m.feasibility_factor("equipment").ok_or("no factor")?;
    /// let bespoke = m.factor_level(eq, "bespoke").ok_or("no level")?;
    /// project.apply(Op::SetStepFactor {
    ///     tree: tree_id, node: step, factor: eq, level: Some(bespoke),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// let rated: Vec<_> = project.step_factors(idx, root).collect();
    /// assert_eq!(rated, [(eq, bespoke)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn step_factors(
        &self,
        tree: TreeIdx,
        node: NodeIdx,
    ) -> impl Iterator<Item = (FeasibilityFactor, FactorLevel)> + '_ {
        let slice = self.step_factor_slice(tree, node);
        self.methodology
            .feasibility_factors()
            .zip(slice)
            .filter_map(|(f, &l)| l.map(|l| (f, l)))
    }

    /// Raw per-factor slice for `node` in `tree` (empty when no factor is
    /// rated on `node`). Time O(1).
    pub(crate) fn step_factor_slice(&self, tree: TreeIdx, node: NodeIdx) -> &[Option<FactorLevel>] {
        self.trees
            .get(tree)
            .and_then(|e| e.step_factors.get(node.index()))
            .map_or(&[][..], Box::as_ref)
    }

    /// Evaluates the complete attack potential breakdown ([`AttackPotential`])
    /// of `node` in `tree` when the methodology declares an `attack_potential`
    /// scheme and all factors are rated on `node`. Returns `None` if `tree` or
    /// `node` is invalid, `node` is a gate, the methodology has no
    /// `attack_potential` scheme, or any factor on `node` is unrated.
    ///
    /// Time O(F), F <= 16; space O(F).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Glitch boot ROM"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// for (f_key, l_key) in [
    ///     ("elapsed-time", "le-1m"),
    ///     ("expertise", "expert"),
    ///     ("knowledge", "restricted"),
    ///     ("window-of-opportunity", "moderate"),
    ///     ("equipment", "specialized"),
    /// ] {
    ///     let f = m.feasibility_factor(f_key).ok_or("no factor")?;
    ///     let l = m.factor_level(f, l_key).ok_or("no level")?;
    ///     project.apply(Op::SetStepFactor {
    ///         tree: tree_id, node: step, factor: f, level: Some(l),
    ///         provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// let ap = project.step_attack_potential(idx, root).ok_or("incomplete")?;
    /// assert_eq!(ap.score(), 21); // 4 + 6 + 3 + 4 + 4
    /// assert_eq!(m.feasibility_level_key(ap.feasibility()), Some("low"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_attack_potential(&self, tree: TreeIdx, node: NodeIdx) -> Option<AttackPotential> {
        self.trees
            .get(tree)?
            .step_attack_potential(&self.methodology, node)
    }

    /// Returns the effective inherent feasibility of `node` in `tree`: if all
    /// attack potential factors are rated on `node`, returns the derived
    /// feasibility from [`step_attack_potential`](Self::step_attack_potential);
    /// otherwise falls back to [`step_feasibility`](Self::step_feasibility).
    ///
    /// Time O(F), F <= 16; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let tree_id = TreeId::new_v7();
    /// let step = NodeId::new_v7();
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// let tree = AttackTree::new(step, Node::step("Step"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, high)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// assert_eq!(project.effective_step_feasibility(idx, root), Some(high));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn effective_step_feasibility(
        &self,
        tree: TreeIdx,
        node: NodeIdx,
    ) -> Option<FeasibilityLevel> {
        self.trees
            .get(tree)?
            .effective_step_feasibility(&self.methodology, node)
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

    /// Iterates the items (components and interfaces) to which `control` is
    /// directly applied (CFG-03, D26), in canonical [`ItemRef`] order.
    /// Returns an empty iterator if `control` is out of range or tombstoned.
    ///
    /// Time O(1) per item, space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, ControlId, ItemRef, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let ecu = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: ecu, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "HSM".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::ApplyControl {
    ///     control: ctrl, target: ItemRef::Component(ecu), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&ctrl).ok_or("missing")?;
    /// let targets: Vec<_> = project.control_targets(idx).collect();
    /// assert_eq!(targets, [ItemRef::Component(ecu)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn control_targets(&self, control: ControlIdx) -> impl Iterator<Item = ItemRef> + '_ {
        self.controls
            .get(control)
            .map_or(&[][..], |c| c.targets.as_slice())
            .iter()
            .filter_map(|&t| self.item_ref(Some(t)))
    }

    /// Iterates live security controls directly applied to `item` (does not
    /// include controls applied to an interface's owning component), in
    /// deterministic `ControlId` order (CFG-03, D26).
    ///
    /// Time O(C * log t) where `C` is the number of live controls and
    /// `t <= 64` is the number of targets on each control, space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, ControlId, ItemRef, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let ecu = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: ecu, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "HSM".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::ApplyControl {
    ///     control: ctrl, target: ItemRef::Component(ecu), provenance: Provenance::default(),
    /// })?;
    /// let applied: Vec<_> = project
    ///     .controls_applied_to(ItemRef::Component(ecu))
    ///     .map(|(_, id, _)| id)
    ///     .collect();
    /// assert_eq!(applied, [ctrl]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn controls_applied_to(
        &self,
        item: ItemRef,
    ) -> impl Iterator<Item = (ControlIdx, ControlId, &SecurityControl)> + '_ {
        let want = self.item_idx(item).ok();
        self.controls
            .iter()
            .filter(move |(_, _, c)| want.is_some_and(|w| c.targets.contains(&w)))
    }

    /// Answers whether the preset `kind` fits `target` (CFG-06): a preset for
    /// components fits an existing component; a preset for interface kinds
    /// fits an existing interface of one of those kinds. False for an unknown
    /// target or an unknown kind. The model only answers; it never refuses.
    ///
    /// Time O(log n + I), `I <= 32`, space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, ItemRef, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let boot = m.control_kind("secure-boot").ok_or("kind")?;
    /// let mut project = Project::new(Arc::clone(&m));
    /// let ecu = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: ecu, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// assert!(project.control_kind_fits(boot, ItemRef::Component(ecu)));
    /// assert!(!project.control_kind_fits(boot, ItemRef::Component(ComponentId::new_v7())));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_kind_fits(&self, kind: ControlKind, target: ItemRef) -> bool {
        if self.methodology.control_kind_key(kind).is_none() {
            return false;
        }
        let kinds = self.methodology.control_kind_interface_kinds(kind);
        match target {
            ItemRef::Component(id) => self.component_idx(&id).is_some() && kinds.is_empty(),
            ItemRef::Interface(id) => self
                .interface_idx(&id)
                .and_then(|idx| self.interface(idx))
                .is_some_and(|i| kinds.contains(&i.kind())),
        }
    }

    /// True when `control` was made from a pack preset and its rules are no
    /// longer the preset's rules in the project's methodology (CFG-06). False
    /// without a kind or for an unknown control.
    ///
    /// Time O(R), `R <= 16`, space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let boot = m.control_kind("secure-boot").ok_or("kind")?;
    /// let mut project = Project::new(m);
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id, name: "Boot".into(), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("idx")?;
    /// assert!(!project.control_differs_from_kind(idx));
    /// project.apply(Op::SetControlKind {
    ///     control: id, kind: Some(boot), provenance: Provenance::default(),
    /// })?;
    /// // The kind alone does not copy the rules, so they differ.
    /// assert!(project.control_differs_from_kind(idx));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control_differs_from_kind(&self, control: ControlIdx) -> bool {
        let Some(c) = self.controls.get(control) else {
            return false;
        };
        c.kind
            .is_some_and(|kind| c.rules.as_slice() != self.methodology.control_kind_rules(kind))
    }

    // -- Cybersecurity goals -----------------------------------------------------

    /// Returns a live cybersecurity goal in O(1), or `None` if `idx` is out of
    /// range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id,
    ///     name: "Ensure firmware authenticity".into(),
    ///     controls: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// assert_eq!(
    ///     project.cybersecurity_goal(idx).map(|g| g.name()),
    ///     Some("Ensure firmware authenticity"),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_goal(&self, idx: GoalIdx) -> Option<&CybersecurityGoal> {
        self.goals.get(idx)
    }

    /// Resolves a cybersecurity goal's public ID to its arena index in
    /// O(log n). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{GoalId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.cybersecurity_goal_idx(&GoalId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_goal_idx(&self, id: &GoalId) -> Option<GoalIdx> {
        self.goals.idx(id)
    }

    /// Returns a live cybersecurity goal's public ID in O(1), or `None` if
    /// `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id,
    ///     name: "Ensure firmware authenticity".into(),
    ///     controls: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.cybersecurity_goal_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_goal_id(&self, idx: GoalIdx) -> Option<GoalId> {
        self.goals.id(idx)
    }

    /// Returns the number of live cybersecurity goals in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.cybersecurity_goal_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_goal_count(&self) -> usize {
        self.goals.len()
    }

    /// Iterates live cybersecurity goals in deterministic `GoalId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (a, b) = (GoalId::new_v7(), GoalId::new_v7());
    /// for id in [b, a] {
    ///     project.apply(Op::AddCybersecurityGoal {
    ///         id,
    ///         name: "Goal".into(),
    ///         controls: vec![],
    ///         provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let ids: Vec<_> = project.cybersecurity_goals().map(|(_, id, _)| id).collect();
    /// assert_eq!(ids, [a.min(b), a.max(b)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn cybersecurity_goals(
        &self,
    ) -> impl Iterator<Item = (GoalIdx, GoalId, &CybersecurityGoal)> {
        self.goals.iter()
    }

    // -- Cybersecurity claims ----------------------------------------------------

    /// Returns a live cybersecurity claim in O(1), or `None` if `idx` is out of
    /// range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ClaimId::new_v7();
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id,
    ///     name: "Physical access requires workshop tools".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
    /// assert_eq!(
    ///     project.cybersecurity_claim(idx).map(|c| c.name()),
    ///     Some("Physical access requires workshop tools"),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_claim(&self, idx: ClaimIdx) -> Option<&CybersecurityClaim> {
        self.claims.get(idx)
    }

    /// Resolves a cybersecurity claim's public ID to its arena index in
    /// O(log n). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ClaimId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.cybersecurity_claim_idx(&ClaimId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_claim_idx(&self, id: &ClaimId) -> Option<ClaimIdx> {
        self.claims.idx(id)
    }

    /// Returns a live cybersecurity claim's public ID in O(1), or `None` if
    /// `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ClaimId::new_v7();
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id,
    ///     name: "Physical access requires workshop tools".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.cybersecurity_claim_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_claim_id(&self, idx: ClaimIdx) -> Option<ClaimId> {
        self.claims.id(idx)
    }

    /// Returns the number of live cybersecurity claims in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.cybersecurity_claim_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_claim_count(&self) -> usize {
        self.claims.len()
    }

    /// Iterates live cybersecurity claims in deterministic `ClaimId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (a, b) = (ClaimId::new_v7(), ClaimId::new_v7());
    /// for id in [b, a] {
    ///     project.apply(Op::AddCybersecurityClaim {
    ///         id,
    ///         name: "Claim".into(),
    ///         provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let ids: Vec<_> = project.cybersecurity_claims().map(|(_, id, _)| id).collect();
    /// assert_eq!(ids, [a.min(b), a.max(b)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn cybersecurity_claims(
        &self,
    ) -> impl Iterator<Item = (ClaimIdx, ClaimId, &CybersecurityClaim)> {
        self.claims.iter()
    }

    // -- Parties -----------------------------------------------------------------

    /// Resolves a party's public ID to its arena index in O(log n).
    /// Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Op, PartyId, PartyKind, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut p = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// # let id = PartyId::new_v7();
    /// # p.apply(Op::AddParty { id, name: "Bosch".into(), kind: PartyKind::Supplier, provenance: Provenance::default() })?;
    /// let idx = p.party_idx(&id).ok_or("missing")?;
    /// assert_eq!(p.party(idx).map(|x| x.name()), Some("Bosch"));
    /// assert_eq!(p.parties().count(), 1);
    /// assert_eq!(p.party_visual_id(idx).as_deref(), Some("P00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn party_idx(&self, id: &PartyId) -> Option<PartyIdx> {
        self.parties.idx(id)
    }

    /// Returns a live party in O(1), or `None` if `idx` is out of range or tombstoned.
    #[must_use]
    pub fn party(&self, idx: PartyIdx) -> Option<&Party> {
        self.parties.get(idx)
    }

    /// Resolves a live party's arena index to its public ID. Time O(1).
    #[must_use]
    pub fn party_id(&self, idx: PartyIdx) -> Option<PartyId> {
        self.parties.id(idx)
    }

    /// Returns a live party's 0-based visual number for snapshot IDs.
    #[must_use]
    pub(crate) fn party_visual_num(&self, idx: PartyIdx) -> Option<u32> {
        self.parties.visual_num(idx)
    }

    /// Iterates live parties in deterministic `PartyId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    pub fn parties(&self) -> impl Iterator<Item = (PartyIdx, PartyId, &Party)> {
        self.parties.iter()
    }

    /// Returns a live party's human-readable visual ID (`P00`, `P01`, ...)
    /// in O(1), or `None` if `idx` is out of range or tombstoned.
    #[must_use]
    pub fn party_visual_id(&self, idx: PartyIdx) -> Option<String> {
        self.parties
            .visual_num(idx)
            .map(|n| VisualPrefix::Party.format(n))
    }

    // -- Components (ARC-02) -----------------------------------------------------

    /// Returns a live component in O(1), or `None` if `idx` is out of range or
    /// tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id,
    ///     name: "Central Gateway".into(),
    ///     parent: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.component_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.component(idx).map(|c| c.name()), Some("Central Gateway"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component(&self, idx: ComponentIdx) -> Option<&Component> {
        self.components.get(idx)
    }

    /// Resolves a component's public ID to its arena index in O(log n).
    /// Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.component_idx(&ComponentId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component_idx(&self, id: &ComponentId) -> Option<ComponentIdx> {
        self.components.idx(id)
    }

    /// Returns a live component's public ID in O(1), or `None` if `idx` is out
    /// of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.component_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.component_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component_id(&self, idx: ComponentIdx) -> Option<ComponentId> {
        self.components.id(idx)
    }

    /// Returns the number of live components in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.component_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Iterates live components in deterministic `ComponentId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (a, b) = (ComponentId::new_v7(), ComponentId::new_v7());
    /// for id in [b, a] {
    ///     project.apply(Op::AddComponent {
    ///         id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let ids: Vec<_> = project.components().map(|(_, id, _)| id).collect();
    /// assert_eq!(ids, [a.min(b), a.max(b)]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn components(&self) -> impl Iterator<Item = (ComponentIdx, ComponentId, &Component)> {
        self.components.iter()
    }

    // -- Interfaces (ARC-02) -----------------------------------------------------

    /// Returns a live interface in O(1), or `None` if `idx` is out of range or
    /// tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN0".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.interface_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.interface(idx).map(|i| i.name()), Some("CAN0"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface(&self, idx: InterfaceIdx) -> Option<&Interface> {
        self.interfaces.get(idx)
    }

    /// Resolves an interface's public ID to its arena index in O(log n).
    /// Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{InterfaceId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.interface_idx(&InterfaceId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface_idx(&self, id: &InterfaceId) -> Option<InterfaceIdx> {
        self.interfaces.idx(id)
    }

    /// Returns a live interface's public ID in O(1), or `None` if `idx` is out
    /// of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN0".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.interface_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.interface_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface_id(&self, idx: InterfaceIdx) -> Option<InterfaceId> {
        self.interfaces.id(idx)
    }

    /// Returns the number of live interfaces in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.interface_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface_count(&self) -> usize {
        self.interfaces.len()
    }

    /// Iterates live interfaces in deterministic `InterfaceId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.interfaces().count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn interfaces(&self) -> impl Iterator<Item = (InterfaceIdx, InterfaceId, &Interface)> {
        self.interfaces.iter()
    }

    // -- Data flows (ARC-02) -----------------------------------------------------

    /// Returns a live data flow in O(1), or `None` if `idx` is out of range or
    /// tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (c1, c2) = (ComponentId::new_v7(), ComponentId::new_v7());
    /// for id in [c1, c2] {
    ///     project.apply(Op::AddComponent {
    ///         id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "CAN frame".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.data_flow_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.data_flow(idx).map(|f| f.name()), Some("CAN frame"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn data_flow(&self, idx: DataFlowIdx) -> Option<&DataFlow> {
        self.data_flows.get(idx)
    }

    /// Resolves a data flow's public ID to its arena index in O(log n).
    /// Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DataFlowId, Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.data_flow_idx(&DataFlowId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn data_flow_idx(&self, id: &DataFlowId) -> Option<DataFlowIdx> {
        self.data_flows.idx(id)
    }

    /// Returns a live data flow's public ID in O(1), or `None` if `idx` is out
    /// of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (c1, c2) = (ComponentId::new_v7(), ComponentId::new_v7());
    /// for id in [c1, c2] {
    ///     project.apply(Op::AddComponent {
    ///         id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "CAN frame".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.data_flow_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.data_flow_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn data_flow_id(&self, idx: DataFlowIdx) -> Option<DataFlowId> {
        self.data_flows.id(idx)
    }

    /// Returns the number of live data flows in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.data_flow_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn data_flow_count(&self) -> usize {
        self.data_flows.len()
    }

    /// Iterates live data flows in deterministic `DataFlowId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.data_flows().count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn data_flows(&self) -> impl Iterator<Item = (DataFlowIdx, DataFlowId, &DataFlow)> {
        self.data_flows.iter()
    }

    // -- Trust boundaries (ARC-02) -----------------------------------------------

    /// Returns a live trust boundary in O(1), or `None` if `idx` is out of
    /// range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Op, Project, Provenance, TrustBoundaryId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.trust_boundary_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.trust_boundary(idx).map(|b| b.name()), Some("Zone A"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn trust_boundary(&self, idx: TrustBoundaryIdx) -> Option<&TrustBoundary> {
        self.trust_boundaries.get(idx)
    }

    /// Resolves a trust boundary's public ID to its arena index in O(log n).
    /// Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, TrustBoundaryId};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.trust_boundary_idx(&TrustBoundaryId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn trust_boundary_idx(&self, id: &TrustBoundaryId) -> Option<TrustBoundaryIdx> {
        self.trust_boundaries.idx(id)
    }

    /// Returns a live trust boundary's public ID in O(1), or `None` if `idx` is
    /// out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Op, Project, Provenance, TrustBoundaryId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.trust_boundary_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.trust_boundary_id(idx), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn trust_boundary_id(&self, idx: TrustBoundaryIdx) -> Option<TrustBoundaryId> {
        self.trust_boundaries.id(idx)
    }

    /// Returns the number of live trust boundaries in O(1). Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.trust_boundary_count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn trust_boundary_count(&self) -> usize {
        self.trust_boundaries.len()
    }

    /// Iterates live trust boundaries in deterministic `TrustBoundaryId` order.
    /// Time O(1) per item, O(n) for the whole walk; space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project};
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.trust_boundaries().count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trust_boundaries(
        &self,
    ) -> impl Iterator<Item = (TrustBoundaryIdx, TrustBoundaryId, &TrustBoundary)> {
        self.trust_boundaries.iter()
    }

    // -- Visual IDs (FND-06) -----------------------------------------------------

    /// Returns a live asset's 0-based visual sequence number in O(1), or
    /// `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id, name: "Brake ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.asset_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.asset_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset_visual_num(&self, idx: AssetIdx) -> Option<u32> {
        self.assets.visual_num(idx)
    }

    /// Returns a live asset's human-readable visual ID (`A00`, `A01`, ...) in
    /// O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id, name: "Brake ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.asset_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.asset_visual_id(idx).as_deref(), Some("A00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset_visual_id(&self, idx: AssetIdx) -> Option<String> {
        self.asset_visual_num(idx)
            .map(|n| VisualPrefix::Asset.format(n))
    }

    /// Returns a live damage scenario's 0-based visual sequence number in
    /// O(1), or `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id, name: "Crash".into(), impacts: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.damage_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.damage_scenario_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damage_scenario_visual_num(&self, idx: DamageIdx) -> Option<u32> {
        self.damages.visual_num(idx)
    }

    /// Returns a live damage scenario's human-readable visual ID (`D00`,
    /// `D01`, ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id, name: "Crash".into(), impacts: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.damage_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.damage_scenario_visual_id(idx).as_deref(), Some("D00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damage_scenario_visual_id(&self, idx: DamageIdx) -> Option<String> {
        self.damage_scenario_visual_num(idx)
            .map(|n| VisualPrefix::Damage.format(n))
    }

    /// Returns a live threat scenario's 0-based visual sequence number in
    /// O(1), or `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let cat = m.threat_category("tampering").ok_or("no cat")?;
    /// let id = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id, name: "Tamper".into(), asset, category: cat, catalog_refs: vec![],
    ///     damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.threat_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.threat_scenario_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_scenario_visual_num(&self, idx: ThreatIdx) -> Option<u32> {
        self.threats.visual_num(idx)
    }

    /// Returns a live threat scenario's human-readable visual ID (`T00`,
    /// `T01`, ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let cat = m.threat_category("tampering").ok_or("no cat")?;
    /// let id = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id, name: "Tamper".into(), asset, category: cat, catalog_refs: vec![],
    ///     damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.threat_scenario_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.threat_scenario_visual_id(idx).as_deref(), Some("T00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn threat_scenario_visual_id(&self, idx: ThreatIdx) -> Option<String> {
        self.threat_scenario_visual_num(idx)
            .map(|n| VisualPrefix::Threat.format(n))
    }

    /// Returns a live attack tree's 0-based visual sequence number in O(1),
    /// or `None` if `idx` is out of range or tombstoned. Space O(1).
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
    /// assert_eq!(project.attack_tree_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree_visual_num(&self, idx: TreeIdx) -> Option<u32> {
        self.trees.visual_num(idx)
    }

    /// Returns a live attack tree's human-readable visual ID (`AT00`, `AT01`,
    /// ...) in O(1), or `None` if `idx` is out of range or tombstoned.
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
    /// assert_eq!(project.attack_tree_visual_id(idx).as_deref(), Some("AT00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree_visual_id(&self, idx: TreeIdx) -> Option<String> {
        self.attack_tree_visual_num(idx)
            .map(|n| VisualPrefix::Tree.format(n))
    }

    /// Returns a live security control's 0-based visual sequence number in
    /// O(1), or `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id, name: "HSM".into(), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.security_control_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn security_control_visual_num(&self, idx: ControlIdx) -> Option<u32> {
        self.controls.visual_num(idx)
    }

    /// Returns a live security control's human-readable visual ID (`SC00`,
    /// `SC01`, ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id, name: "HSM".into(), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.security_control_visual_id(idx).as_deref(), Some("SC00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn security_control_visual_id(&self, idx: ControlIdx) -> Option<String> {
        self.security_control_visual_num(idx)
            .map(|n| VisualPrefix::Control.format(n))
    }

    /// Returns a live cybersecurity goal's 0-based visual sequence number in
    /// O(1), or `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id, name: "Auth".into(), controls: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.cybersecurity_goal_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_goal_visual_num(&self, idx: GoalIdx) -> Option<u32> {
        self.goals.visual_num(idx)
    }

    /// Returns a live cybersecurity goal's human-readable visual ID (`G00`,
    /// `G01`, ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id, name: "Auth".into(), controls: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.cybersecurity_goal_visual_id(idx).as_deref(), Some("G00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_goal_visual_id(&self, idx: GoalIdx) -> Option<String> {
        self.cybersecurity_goal_visual_num(idx)
            .map(|n| VisualPrefix::Goal.format(n))
    }

    /// Returns a live cybersecurity claim's 0-based visual sequence number in
    /// O(1), or `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ClaimId::new_v7();
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id, name: "Physical".into(), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.cybersecurity_claim_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_claim_visual_num(&self, idx: ClaimIdx) -> Option<u32> {
        self.claims.visual_num(idx)
    }

    /// Returns a live cybersecurity claim's human-readable visual ID (`CL00`,
    /// `CL01`, ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ClaimId::new_v7();
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id, name: "Physical".into(), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.cybersecurity_claim_visual_id(idx).as_deref(), Some("CL00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn cybersecurity_claim_visual_id(&self, idx: ClaimIdx) -> Option<String> {
        self.cybersecurity_claim_visual_num(idx)
            .map(|n| VisualPrefix::Claim.format(n))
    }

    /// Returns a live component's 0-based visual sequence number in O(1), or
    /// `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.component_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.component_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component_visual_num(&self, idx: ComponentIdx) -> Option<u32> {
        self.components.visual_num(idx)
    }

    /// Returns a live component's human-readable visual ID (`C00`, `C01`, ...)
    /// in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.component_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.component_visual_id(idx).as_deref(), Some("C00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component_visual_id(&self, idx: ComponentIdx) -> Option<String> {
        self.component_visual_num(idx)
            .map(|n| VisualPrefix::Component.format(n))
    }

    /// Returns a live interface's 0-based visual sequence number in O(1), or
    /// `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.interface_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.interface_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface_visual_num(&self, idx: InterfaceIdx) -> Option<u32> {
        self.interfaces.visual_num(idx)
    }

    /// Returns a live interface's human-readable visual ID (`IF00`, `IF01`,
    /// ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let idx = project.interface_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.interface_visual_id(idx).as_deref(), Some("IF00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface_visual_id(&self, idx: InterfaceIdx) -> Option<String> {
        self.interface_visual_num(idx)
            .map(|n| VisualPrefix::Interface.format(n))
    }

    /// Returns a live data flow's 0-based visual sequence number in O(1), or
    /// `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (c1, c2) = (ComponentId::new_v7(), ComponentId::new_v7());
    /// for id in [c1, c2] {
    ///     project.apply(Op::AddComponent {
    ///         id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "CAN".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.data_flow_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.data_flow_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn data_flow_visual_num(&self, idx: DataFlowIdx) -> Option<u32> {
        self.data_flows.visual_num(idx)
    }

    /// Returns a live data flow's human-readable visual ID (`DF00`, `DF01`,
    /// ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let (c1, c2) = (ComponentId::new_v7(), ComponentId::new_v7());
    /// for id in [c1, c2] {
    ///     project.apply(Op::AddComponent {
    ///         id, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    ///     })?;
    /// }
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "CAN".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.data_flow_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.data_flow_visual_id(idx).as_deref(), Some("DF00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn data_flow_visual_id(&self, idx: DataFlowIdx) -> Option<String> {
        self.data_flow_visual_num(idx)
            .map(|n| VisualPrefix::DataFlow.format(n))
    }

    /// Returns a live trust boundary's 0-based visual sequence number in
    /// O(1), or `None` if `idx` is out of range or tombstoned. Space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Op, Project, Provenance, TrustBoundaryId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.trust_boundary_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.trust_boundary_visual_num(idx), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn trust_boundary_visual_num(&self, idx: TrustBoundaryIdx) -> Option<u32> {
        self.trust_boundaries.visual_num(idx)
    }

    /// Returns a live trust boundary's human-readable visual ID (`TB00`,
    /// `TB01`, ...) in O(1), or `None` if `idx` is out of range or tombstoned.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Op, Project, Provenance, TrustBoundaryId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![], provenance: Provenance::default(),
    /// })?;
    /// let idx = project.trust_boundary_idx(&id).ok_or("missing")?;
    /// assert_eq!(project.trust_boundary_visual_id(idx).as_deref(), Some("TB00"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn trust_boundary_visual_id(&self, idx: TrustBoundaryIdx) -> Option<String> {
        self.trust_boundary_visual_num(idx)
            .map(|n| VisualPrefix::TrustBoundary.format(n))
    }

    /// Returns the human-readable visual ID of any live [`ElementRef`] in
    /// O(log n), or `None` for [`ElementRef::Project`] or a missing ID.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, ElementRef, Methodology, Op, Project, Provenance};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.visual_id(ElementRef::Asset(id)).as_deref(), Some("A00"));
    /// assert_eq!(project.visual_id(ElementRef::Project), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn visual_id(&self, target: ElementRef) -> Option<String> {
        match target {
            ElementRef::Project | ElementRef::ItemDescription | ElementRef::ItemScope => None,
            ElementRef::Asset(id) => self
                .asset_idx(&id)
                .and_then(|idx| self.asset_visual_id(idx)),
            ElementRef::Damage(id) => self
                .damage_scenario_idx(&id)
                .and_then(|idx| self.damage_scenario_visual_id(idx)),
            ElementRef::Threat(id) => self
                .threat_scenario_idx(&id)
                .and_then(|idx| self.threat_scenario_visual_id(idx)),
            ElementRef::Tree(id) => self
                .attack_tree_idx(&id)
                .and_then(|idx| self.attack_tree_visual_id(idx)),
            ElementRef::Control(id) => self
                .security_control_idx(&id)
                .and_then(|idx| self.security_control_visual_id(idx)),
            ElementRef::Goal(id) => self
                .cybersecurity_goal_idx(&id)
                .and_then(|idx| self.cybersecurity_goal_visual_id(idx)),
            ElementRef::Claim(id) => self
                .cybersecurity_claim_idx(&id)
                .and_then(|idx| self.cybersecurity_claim_visual_id(idx)),
            ElementRef::Component(id) => self
                .component_idx(&id)
                .and_then(|idx| self.component_visual_id(idx)),
            ElementRef::Interface(id) => self
                .interface_idx(&id)
                .and_then(|idx| self.interface_visual_id(idx)),
            ElementRef::DataFlow(id) => self
                .data_flow_idx(&id)
                .and_then(|idx| self.data_flow_visual_id(idx)),
            ElementRef::TrustBoundary(id) => self
                .trust_boundary_idx(&id)
                .and_then(|idx| self.trust_boundary_visual_id(idx)),
        }
    }

    pub(crate) fn set_asset_loaded_visual(&mut self, id: &AssetId, vis: u32, next: u32) {
        self.assets.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_damage_loaded_visual(&mut self, id: &DamageId, vis: u32, next: u32) {
        self.damages.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_threat_loaded_visual(&mut self, id: &ThreatId, vis: u32, next: u32) {
        self.threats.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_tree_loaded_visual(&mut self, id: &TreeId, vis: u32, next: u32) {
        self.trees.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_control_loaded_visual(&mut self, id: &ControlId, vis: u32, next: u32) {
        self.controls.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_goal_loaded_visual(&mut self, id: &GoalId, vis: u32, next: u32) {
        self.goals.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_claim_loaded_visual(&mut self, id: &ClaimId, vis: u32, next: u32) {
        self.claims.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_party_loaded_visual(&mut self, id: &PartyId, vis: u32, next: u32) {
        self.parties.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_component_loaded_visual(&mut self, id: &ComponentId, vis: u32, next: u32) {
        self.components.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_interface_loaded_visual(&mut self, id: &InterfaceId, vis: u32, next: u32) {
        self.interfaces.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_data_flow_loaded_visual(&mut self, id: &DataFlowId, vis: u32, next: u32) {
        self.data_flows.set_loaded_visual(id, vis, next);
    }

    pub(crate) fn set_trust_boundary_loaded_visual(
        &mut self,
        id: &TrustBoundaryId,
        vis: u32,
        next: u32,
    ) {
        self.trust_boundaries.set_loaded_visual(id, vis, next);
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

    /// The component or interface the step `node` of `tree` goes through, if
    /// any (CFG-01). O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ComponentId, GateKind, ItemRef, Methodology, Node, NodeId, Op, Project,
    /// #     Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let ecu = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: ecu, name: "ADAS ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let (tree_id, step) = (TreeId::new_v7(), NodeId::new_v7());
    /// let tree = AttackTree::new(step, Node::step("Flash firmware"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepVia {
    ///     tree: tree_id, node: step, via: Some(ItemRef::Component(ecu)),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// let root = project.attack_tree(tree_idx).ok_or("missing")?.root();
    /// assert_eq!(project.step_via(tree_idx, root), Some(ItemRef::Component(ecu)));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_via(&self, tree: TreeIdx, node: NodeIdx) -> Option<ItemRef> {
        self.item_ref(self.trees.get(tree)?.via_at(node.index()))
    }

    /// The attack type of the step `node` of `tree`, if any (CFG-02). O(1).
    #[must_use]
    pub fn step_attack_type(&self, tree: TreeIdx, node: NodeIdx) -> Option<AttackType> {
        self.trees.get(tree)?.attack_type_at(node.index())
    }

    /// Every step that goes through `item` itself (a component's list does
    /// not include steps through its interfaces), in tree then node order.
    /// O(total nodes).
    pub fn steps_via(&self, item: ItemRef) -> impl Iterator<Item = (TreeIdx, NodeIdx)> + '_ {
        let want = self.item_idx(item).ok();
        self.trees
            .iter()
            .filter(move |_| want.is_some())
            .flat_map(move |(t, _, e)| {
                e.vias()
                    .filter(move |(_, v)| Some(*v) == want)
                    .filter_map(move |(i, _)| Some((t, NodeIdx::try_from_usize(i)?)))
            })
    }

    /// Returns the residual feasibility of `node` in `tree` — the minimum of
    /// its inherent [`effective_step_feasibility`](Self::effective_step_feasibility),
    /// all attached [`step_mitigations`](Self::step_mitigations), and every
    /// matching [`ControlRule`] of every [`SecurityControl`] applied to the
    /// step's [`step_via`](Self::step_via) item or (when `via` is an
    /// interface) to its owning component (CFG-05, D26) — or `None` if `tree`
    /// or `node` is invalid, `node` is a gate, or the step has no inherent
    /// feasibility rating.
    ///
    /// Time O(F + m + C) where `F <= 16` is the number of attack potential
    /// factors, `m <= 16` is the number of mitigations on `node`, and `C` is
    /// the number of live controls when the step's `via` (or its component)
    /// has applied controls (O(F + m) otherwise). Space O(1).
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
        let entry = self.trees.get(tree)?;
        self.entry_residual_step_feasibility(entry, node)
    }

    /// Returns every active feasibility ceiling on step `node` in `tree` —
    /// both manual [`step_mitigations`](Self::step_mitigations) and matching
    /// [`ControlRule`]s from controls applied to the step's
    /// [`step_via`](Self::step_via) item or its owning component — ordered
    /// with the deciding (lowest-feasibility) ceiling first (CFG-05, D26).
    ///
    /// Ties on [`StepCeiling::feasibility`] are broken deterministically by
    /// `(control_id, source)`. Returns an empty `Vec` if `tree` or `node` is
    /// invalid or `node` is a gate.
    ///
    /// Time O(m + C + k log k) where `m <= 16` is the number of manual
    /// mitigations, `C` is the number of live controls, and `k` is the number
    /// of active ceilings on `node`. Space O(k).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, CeilingSource, ComponentId, ControlId, ItemRef, Methodology, Node,
    /// #     NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let (low, med, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let spoof = m.attack_types().next().ok_or("no attack type")?;
    /// let ecu = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: ecu, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "SecOC".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetControlRule {
    ///     control: ctrl, attack_type: spoof, ceiling: Some(low), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::ApplyControl {
    ///     control: ctrl, target: ItemRef::Component(ecu), provenance: Provenance::default(),
    /// })?;
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Inject frame"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepVia {
    ///     tree: tree_id, node: step, via: Some(ItemRef::Component(ecu)), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepAttackType {
    ///     tree: tree_id, node: step, attack_type: Some(spoof), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(med), provenance: Provenance::default(),
    /// })?;
    /// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// let ceilings = project.step_ceilings(tree_idx, root);
    /// assert_eq!(ceilings.len(), 2);
    /// assert_eq!(ceilings[0].feasibility(), low);
    /// assert_eq!(
    ///     ceilings[0].source(),
    ///     CeilingSource::ControlRule { target: ItemRef::Component(ecu), attack_type: spoof },
    /// );
    /// assert_eq!(ceilings[1].feasibility(), med);
    /// assert_eq!(ceilings[1].source(), CeilingSource::StepMitigation);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn step_ceilings(&self, tree: TreeIdx, node: NodeIdx) -> Vec<StepCeiling> {
        let Some(entry) = self.trees.get(tree) else {
            return Vec::new();
        };
        if !matches!(entry.tree.get(node), Some(Node::Step { .. })) {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(mitigations) = entry.step_mitigations.get(node.index()) {
            for m in mitigations {
                if let Some(control_id) = self.control_key(m.control) {
                    out.push(StepCeiling {
                        control: m.control,
                        control_id,
                        feasibility: m.feasibility,
                        source: CeilingSource::StepMitigation,
                    });
                }
            }
        }
        if let (Some(via), Some(attack_type)) = (
            entry.via_at(node.index()),
            entry.attack_type_at(node.index()),
        ) {
            let owner = self.item_owner_component(via);
            if self.item_control_count(via) > 0
                || owner.is_some_and(|o| self.item_control_count(o) > 0)
            {
                let via_ref = self.item_ref(Some(via));
                let owner_ref = owner.and_then(|o| self.item_ref(Some(o)));
                for (ctrl_idx, ctrl_id, ctrl) in self.controls.iter() {
                    let Some(feasibility) = ctrl.rule_for(attack_type) else {
                        continue;
                    };
                    if let Some(target) = via_ref.filter(|_| ctrl.targets.contains(&via)) {
                        out.push(StepCeiling {
                            control: ctrl_idx,
                            control_id: ctrl_id,
                            feasibility,
                            source: CeilingSource::ControlRule {
                                target,
                                attack_type,
                            },
                        });
                    }
                    if let (Some(o_idx), Some(target)) = (owner, owner_ref)
                        && ctrl.targets.contains(&o_idx)
                    {
                        out.push(StepCeiling {
                            control: ctrl_idx,
                            control_id: ctrl_id,
                            feasibility,
                            source: CeilingSource::ControlRule {
                                target,
                                attack_type,
                            },
                        });
                    }
                }
            }
        }
        out.sort_unstable_by_key(|c| (c.feasibility, c.control_id, c.source));
        out
    }

    /// Returns the attack step that decides the residual feasibility of
    /// `tree`, with the ceilings that lower it (CFG-07).
    ///
    /// `residual` must be the result of
    /// [`evaluate_residual_tree`](Self::evaluate_residual_tree) for the same
    /// tree. The step is where the deciding chain ends. Only ceilings below
    /// the step's own rating are listed, the lowest first.
    ///
    /// Returns `None` if `tree` is not a live tree, if `residual` has a
    /// different node count than the tree (it belongs to another tree), or if
    /// the chain does not end at an attack step.
    ///
    /// Time O(depth + controls), space O(ceilings on the step).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ComponentId, ControlId, ItemRef, Methodology, Node, NodeId, Op,
    /// #     Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
    /// let (vlow, high) = (
    ///     m.feasibility_level("very-low").ok_or("no very-low")?,
    ///     m.feasibility_level("high").ok_or("no high")?,
    /// );
    /// let mut project = Project::new(m);
    /// let comp = ComponentId::new_v7();
    /// let ctrl = ControlId::new_v7();
    /// let (tree_id, step) = (TreeId::new_v7(), NodeId::new_v7());
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "ADAS ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "JTAG disabled".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::ApplyControl {
    ///     control: ctrl, target: ItemRef::Component(comp), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetControlRule {
    ///     control: ctrl, attack_type: phys, ceiling: Some(vlow), provenance: Provenance::default(),
    /// })?;
    /// let tree = AttackTree::new(step, Node::step("Dump flash"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepVia {
    ///     tree: tree_id, node: step, via: Some(ItemRef::Component(comp)),
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepAttackType {
    ///     tree: tree_id, node: step, attack_type: Some(phys), provenance: Provenance::default(),
    /// })?;
    /// let t_idx = project.attack_tree_idx(&tree_id).ok_or("no tree")?;
    /// let residual = project.evaluate_residual_tree(tree_id)?;
    /// let d = project.deciding_step(t_idx, &residual).ok_or("no deciding step")?;
    /// assert_eq!(d.node_id(), step);
    /// assert_eq!(d.rated(), high);
    /// assert_eq!(d.residual(), vlow);
    /// assert_eq!(d.ceilings().len(), 1);
    /// assert!(d.is_capped());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn deciding_step(
        &self,
        tree: TreeIdx,
        residual: &Evaluation<FeasibilityLevel>,
    ) -> Option<DecidingStep> {
        let entry = self.trees.get(tree)?;
        if residual.len() != entry.tree.len() {
            return None;
        }
        let node = residual.why_path(entry.tree.root()).last()?;
        if !matches!(entry.tree.get(node), Some(Node::Step { .. })) {
            return None;
        }
        let node_id = entry.tree.id_of(node)?;
        let rated = self.effective_step_feasibility(tree, node)?;
        let residual_level = self.residual_step_feasibility(tree, node)?;
        let ceilings = self
            .step_ceilings(tree, node)
            .into_iter()
            .filter(|c| c.feasibility() < rated)
            .collect();
        Some(DecidingStep {
            tree,
            node,
            node_id,
            rated,
            residual: residual_level,
            ceilings,
        })
    }

    fn item_owner_component(&self, item: ItemIdx) -> Option<ItemIdx> {
        match item {
            ItemIdx::Component(_) => None,
            ItemIdx::Interface(i) => self
                .interfaces
                .get(i)
                .map(|iface| ItemIdx::Component(iface.component)),
        }
    }

    fn item_control_count(&self, item: ItemIdx) -> u32 {
        match item {
            ItemIdx::Component(c) => self
                .refs
                .component_controls
                .get(c.index())
                .copied()
                .unwrap_or(0),
            ItemIdx::Interface(i) => self
                .refs
                .interface_controls
                .get(i.index())
                .copied()
                .unwrap_or(0),
        }
    }

    fn entry_step_has_rule_ceiling(&self, entry: &TreeEntry, step_pos: usize) -> bool {
        let (Some(via), Some(attack_type)) =
            (entry.via_at(step_pos), entry.attack_type_at(step_pos))
        else {
            return false;
        };
        let owner = self.item_owner_component(via);
        if self.item_control_count(via) == 0
            && owner.is_none_or(|o| self.item_control_count(o) == 0)
        {
            return false;
        }
        self.controls.iter().any(|(_, _, ctrl)| {
            (ctrl.targets.contains(&via) || owner.is_some_and(|o| ctrl.targets.contains(&o)))
                && ctrl.rule_for(attack_type).is_some()
        })
    }

    fn entry_residual_step_feasibility(
        &self,
        entry: &TreeEntry,
        idx: NodeIdx,
    ) -> Option<FeasibilityLevel> {
        let mut res = entry.effective_step_feasibility(&self.methodology, idx)?;
        if let Some(mitigations) = entry.step_mitigations.get(idx.index()) {
            for m in mitigations {
                res = res.min(m.feasibility);
            }
        }
        if let (Some(via), Some(attack_type)) =
            (entry.via_at(idx.index()), entry.attack_type_at(idx.index()))
        {
            let owner = self.item_owner_component(via);
            if self.item_control_count(via) > 0
                || owner.is_some_and(|o| self.item_control_count(o) > 0)
            {
                for (_, _, ctrl) in self.controls.iter() {
                    if (ctrl.targets.contains(&via)
                        || owner.is_some_and(|o| ctrl.targets.contains(&o)))
                        && let Some(ceiling) = ctrl.rule_for(attack_type)
                    {
                        res = res.min(ceiling);
                    }
                }
            }
        }
        Some(res)
    }

    /// Evaluates attack tree `id` bottom-up using
    /// [`MaxMin<FeasibilityLevel>`] and each step's
    /// [`effective_step_feasibility`](Self::effective_step_feasibility).
    ///
    /// Time O(V * F + E), space O(V).
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
            entry.effective_step_feasibility(&self.methodology, idx)
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
            self.entry_residual_step_feasibility(entry, idx)
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

    /// Evaluates attack tree `id` using inherent step feasibility ratings and
    /// ranks its leaf steps by how many of the top-`k` easiest minimal attack
    /// paths each one lies on (rule A4).
    ///
    /// Time O((V + E) + k * V * (V + E) * log(k * d)), space
    /// O(V + k * d * V).
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
    /// let (med, high) = (
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let (root, x, or_id, y, z) = (
    ///     NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7(),
    /// );
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::And, "Goal"))?;
    /// let ix = tree.add_child(tree.root(), x, Node::step("X"))?;
    /// let i_or = tree.add_child(tree.root(), or_id, Node::gate(GateKind::Or, "Or"))?;
    /// tree.add_child(i_or, y, Node::step("Y"))?;
    /// tree.add_child(i_or, z, Node::step("Z"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id,
    ///     tree,
    ///     step_feasibility: vec![(x, high), (y, med), (z, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let choke = project.tree_choke_points(tree_id, 2)?;
    /// assert_eq!(choke[0].step(), ix);
    /// assert_eq!(choke[0].path_count(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn tree_choke_points(
        &self,
        id: TreeId,
        k: usize,
    ) -> Result<Vec<ChokePoint<FeasibilityLevel>>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let eval = self.evaluate_tree(id)?;
        Ok(eval.choke_points(&entry.tree, k).unwrap_or_default())
    }

    /// Evaluates attack tree `id` using residual step feasibility ratings and
    /// ranks its leaf steps by how many of the top-`k` easiest residual
    /// minimal attack paths each one lies on (rule A4).
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
    /// tree.add_child(tree.root(), obd, Node::step("OBD"))?;
    /// let i_ota = tree.add_child(tree.root(), ota, Node::step("OTA"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(obd, high), (ota, med)],
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
    /// let choke = project.residual_tree_choke_points(tree_id, 1)?;
    /// assert_eq!(choke.len(), 1);
    /// assert_eq!(choke[0].step(), i_ota);
    /// # Ok(())
    /// # }
    /// ```
    pub fn residual_tree_choke_points(
        &self,
        id: TreeId,
        k: usize,
    ) -> Result<Vec<ChokePoint<FeasibilityLevel>>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let eval = self.evaluate_residual_tree(id)?;
        Ok(eval.choke_points(&entry.tree, k).unwrap_or_default())
    }

    /// Evaluates the attack tree linked to threat scenario `id` using inherent
    /// step feasibility ratings and ranks its leaf steps by how many of the
    /// top-`k` easiest minimal attack paths each one lies on (rule A4).
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
    /// let choke = project.threat_choke_points(threat, 5)?;
    /// assert_eq!(choke.len(), 1);
    /// assert_eq!(choke[0].path_count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn threat_choke_points(
        &self,
        id: ThreatId,
        k: usize,
    ) -> Result<Vec<ChokePoint<FeasibilityLevel>>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.tree_choke_points(tree_id, k)
    }

    /// Evaluates the attack tree linked to threat scenario `id` using residual
    /// step feasibility ratings and ranks its leaf steps by how many of the
    /// top-`k` easiest residual minimal attack paths each one lies on (rule A4).
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
    /// let choke = project.residual_threat_choke_points(threat, 5)?;
    /// assert_eq!(choke.len(), 1);
    /// assert_eq!(*choke[0].easiest_path_feasibility(), low);
    /// # Ok(())
    /// # }
    /// ```
    pub fn residual_threat_choke_points(
        &self,
        id: ThreatId,
        k: usize,
    ) -> Result<Vec<ChokePoint<FeasibilityLevel>>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.residual_tree_choke_points(tree_id, k)
    }

    /// Evaluates attack tree `id` using inherent step feasibility ratings and
    /// returns a greedy hitting set of leaf steps covering all of the top-`k`
    /// easiest minimal attack paths (rule A4; see
    /// [`Evaluation::top_k_hitting_set`]).
    ///
    /// Time O((V + E) + k * V * (V + E) * log(k * d)), space
    /// O(V + k * d * V).
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
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// let (root, x, y) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::And, "Goal"))?;
    /// let ix = tree.add_child(tree.root(), x, Node::step("X"))?;
    /// tree.add_child(tree.root(), y, Node::step("Y"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(x, high), (y, high)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.tree_hitting_set(tree_id, 5)?, vec![ix]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn tree_hitting_set(&self, id: TreeId, k: usize) -> Result<Vec<NodeIdx>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let eval = self.evaluate_tree(id)?;
        Ok(eval.top_k_hitting_set(&entry.tree, k).unwrap_or_default())
    }

    /// Evaluates attack tree `id` using residual step feasibility ratings and
    /// returns a greedy hitting set of leaf steps covering all of the top-`k`
    /// easiest residual minimal attack paths (rule A4).
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
    /// tree.add_child(tree.root(), obd, Node::step("OBD"))?;
    /// let i_ota = tree.add_child(tree.root(), ota, Node::step("OTA"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(obd, high), (ota, med)],
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
    /// assert_eq!(project.residual_tree_hitting_set(tree_id, 1)?, vec![i_ota]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn residual_tree_hitting_set(
        &self,
        id: TreeId,
        k: usize,
    ) -> Result<Vec<NodeIdx>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let eval = self.evaluate_residual_tree(id)?;
        Ok(eval.top_k_hitting_set(&entry.tree, k).unwrap_or_default())
    }

    /// Evaluates the attack tree linked to threat scenario `id` using inherent
    /// step feasibility ratings and returns a greedy hitting set of leaf steps
    /// covering all of the top-`k` easiest minimal attack paths (rule A4).
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
    /// let tree = AttackTree::new(step, Node::step("Step"))?;
    /// let root_idx = tree.root();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.threat_hitting_set(threat, 5)?, vec![root_idx]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn threat_hitting_set(&self, id: ThreatId, k: usize) -> Result<Vec<NodeIdx>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.tree_hitting_set(tree_id, k)
    }

    /// Evaluates the attack tree linked to threat scenario `id` using residual
    /// step feasibility ratings and returns a greedy hitting set of leaf steps
    /// covering all of the top-`k` easiest residual minimal attack paths
    /// (rule A4).
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
    /// let tree = AttackTree::new(step, Node::step("Replay"))?;
    /// let root_idx = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(step, high)],
    ///     provenance: Provenance::default(),
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
    /// assert_eq!(project.residual_threat_hitting_set(threat, 5)?, vec![root_idx]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn residual_threat_hitting_set(
        &self,
        id: ThreatId,
        k: usize,
    ) -> Result<Vec<NodeIdx>, ModelError> {
        let t = self.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = self
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.residual_tree_hitting_set(tree_id, k)
    }

    /// Identifies every leaf step of attack tree `id` where a $\pm 1$ move on
    /// the methodology's feasibility scale changes the tree's inherent root
    /// feasibility.
    ///
    /// Uses [`TreeEvalCache`] so each candidate $\pm 1$ perturbation
    /// recomputes only the dirty ancestors of that step in
    /// $O(\text{depth} \times \text{fan-out})$ time (rule A1).
    ///
    /// Time O((V + E) + S * depth * fan-out) where `S <= V` is the number of
    /// leaf steps, space O(V).
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
    /// let (low, med, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let (root, s1, s2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Goal"))?;
    /// tree.add_child(tree.root(), s1, Node::step("Hard"))?;
    /// tree.add_child(tree.root(), s2, Node::step("Dominant"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(s1, low), (s2, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// // Only `s2` (medium) is a tipping point: moving it to `low` or `high` changes the root.
    /// assert_eq!(tips.len(), 2);
    /// assert!(tips.iter().all(|t| t.node() == s2));
    /// assert_eq!((tips[0].to_step(), tips[0].to_root()), (low, low));
    /// assert_eq!((tips[1].to_step(), tips[1].to_root()), (high, high));
    /// # Ok(())
    /// # }
    /// ```
    pub fn tree_tipping_points(&self, id: TreeId) -> Result<Vec<StepTippingPoint>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let (_, tips) = self.entry_tipping_points(entry, false)?;
        Ok(tips)
    }

    /// Identifies every leaf step of attack tree `id` where a $\pm 1$ move on
    /// the methodology's feasibility scale from the step's effective residual
    /// feasibility changes the tree's residual root feasibility.
    ///
    /// Time O((V * m + E) + S * depth * fan-out * m), space O(V).
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
    /// let (very_low, low, med, high) = (
    ///     m.feasibility_level("very-low").ok_or("no level")?,
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let (root, obd, ota) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise"))?;
    /// tree.add_child(tree.root(), obd, Node::step("OBD"))?;
    /// tree.add_child(tree.root(), ota, Node::step("OTA"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(obd, high), (ota, med)],
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
    /// // Residually, `ota` (medium) dominates `obd` (very-low), so `ota` is the tipping point.
    /// let tips = project.residual_tree_tipping_points(tree_id)?;
    /// assert_eq!(tips.len(), 2);
    /// assert!(tips.iter().all(|t| t.node() == ota));
    /// assert_eq!((tips[0].to_step(), tips[0].to_root()), (low, low));
    /// assert_eq!((tips[1].to_step(), tips[1].to_root()), (high, high));
    /// # Ok(())
    /// # }
    /// ```
    pub fn residual_tree_tipping_points(
        &self,
        id: TreeId,
    ) -> Result<Vec<StepTippingPoint>, ModelError> {
        let (_, entry) = self.live_tree(id)?;
        let (_, tips) = self.entry_tipping_points(entry, true)?;
        Ok(tips)
    }

    /// Computes the inherent single-step ($\pm 1$) tipping-point sensitivity
    /// of threat scenario `id` across its feasibility source (linked attack
    /// tree leaf steps or direct threat feasibility) and all rated impact
    /// categories on its linked damage scenarios.
    ///
    /// Time O(log n + (V + E) + S * (depth * fan-out + d * c) + d * c), space
    /// O(V + S + d * c).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate_risk`](Self::evaluate_risk).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, GateKind, Methodology, Node, NodeId, Op, Project,
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
    /// let major = m.level(safety, "major").ok_or("no level")?;
    /// let damage = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario {
    ///     id: damage, name: "Brake loss".into(), impacts: vec![(safety, major)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Spoof".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
    /// })?;
    /// let (med, high) = (
    ///     m.feasibility_level("medium").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let (root, s1, s2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::And, "Goal"))?;
    /// tree.add_child(tree.root(), s1, Node::step("Bottleneck"))?;
    /// tree.add_child(tree.root(), s2, Node::step("Easy"))?;
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![(s1, med), (s2, high)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.max_risk(), m.risk_level("r3"));
    /// assert!(!sens.is_stable());
    /// assert_eq!(sens.feasibility_tipping_points().len(), 2);
    /// assert_eq!(sens.impact_tipping_points().len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn threat_sensitivity(&self, id: ThreatId) -> Result<ThreatSensitivity, ModelError> {
        self.threat_sensitivity_inner(id, false)
    }

    /// Computes the residual single-step ($\pm 1$) tipping-point sensitivity
    /// of threat scenario `id`, accounting for all [`StepMitigation`]s on its
    /// linked attack tree.
    ///
    /// Time O(log n + (V * m + E) + S * (depth * fan-out * m + d * c) + d * c),
    /// space O(V + S + d * c).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate_residual_risk`](Self::evaluate_residual_risk).
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
    /// let (very_low, low, high) = (
    ///     m.feasibility_level("very-low").ok_or("no level")?,
    ///     m.feasibility_level("low").ok_or("no level")?,
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
    /// let sens = project.residual_threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility(), very_low);
    /// assert_eq!(sens.max_risk(), m.risk_level("r2"));
    /// assert_eq!(sens.feasibility_tipping_points().len(), 1);
    /// assert_eq!(sens.feasibility_tipping_points()[0].to_step_feasibility(), low);
    /// assert_eq!(sens.feasibility_tipping_points()[0].to_risk(), m.risk_level("r3").ok_or("no r3")?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn residual_threat_sensitivity(
        &self,
        id: ThreatId,
    ) -> Result<ThreatSensitivity, ModelError> {
        self.threat_sensitivity_inner(id, true)
    }

    fn entry_tipping_points(
        &self,
        entry: &TreeEntry,
        residual: bool,
    ) -> Result<(FeasibilityLevel, Vec<StepTippingPoint>), ModelError> {
        let domain = MaxMin::<FeasibilityLevel>::new();
        let mut cache = TreeEvalCache::new(&entry.tree, &domain, |idx| {
            if residual {
                self.entry_residual_step_feasibility(entry, idx)
            } else {
                entry.effective_step_feasibility(&self.methodology, idx)
            }
        })?;
        let baseline_root = *cache.evaluation().root_value();
        let feas_count = self.methodology.feasibility_levels().len();
        let mut out = Vec::new();
        for (step_idx, node_id, node) in entry.tree.iter() {
            if !matches!(node, Node::Step { .. }) {
                continue;
            }
            let Some(&from_step) = cache.evaluation().get(step_idx) else {
                continue;
            };
            let neighbors = adjacent_feasibility(from_step, feas_count);
            for to_step in neighbors.into_iter().flatten() {
                cache.recompute_steps(&entry.tree, &domain, [step_idx], |idx| {
                    if idx == step_idx {
                        Some(to_step)
                    } else if residual {
                        self.entry_residual_step_feasibility(entry, idx)
                    } else {
                        entry.effective_step_feasibility(&self.methodology, idx)
                    }
                })?;
                let to_root = *cache.evaluation().root_value();
                cache.recompute_steps(&entry.tree, &domain, [step_idx], |idx| {
                    if idx == step_idx {
                        Some(from_step)
                    } else if residual {
                        self.entry_residual_step_feasibility(entry, idx)
                    } else {
                        entry.effective_step_feasibility(&self.methodology, idx)
                    }
                })?;
                if to_root != baseline_root {
                    out.push(StepTippingPoint {
                        step: step_idx,
                        node: node_id,
                        from_step,
                        to_step,
                        from_root: baseline_root,
                        to_root,
                    });
                }
            }
        }
        Ok((baseline_root, out))
    }

    fn max_risk_for_feasibility(
        &self,
        t: &ThreatScenario,
        feasibility: FeasibilityLevel,
    ) -> Option<RiskLevel> {
        let mut best: Option<RiskLevel> = None;
        for &damage_idx in &t.damages {
            let Some(damage) = self.damages.get(damage_idx) else {
                continue;
            };
            for (category, impact) in damage.impacts() {
                let Some(impact) = impact else { continue };
                let Some(risk) = self.methodology.risk_for(category, impact, feasibility) else {
                    continue;
                };
                if best.is_none_or(|b| risk > b) {
                    best = Some(risk);
                }
            }
        }
        best
    }

    fn threat_sensitivity_inner(
        &self,
        id: ThreatId,
        residual: bool,
    ) -> Result<ThreatSensitivity, ModelError> {
        let t = self.threat_ref(id)?;
        let feas_count = self.methodology.feasibility_levels().len();
        let mut feasibility_tipping_points = Vec::new();

        let (baseline_feasibility, baseline_risk) = if let Some(tree_idx) = t.attack_tree {
            let entry = self
                .trees
                .get(tree_idx)
                .ok_or(ModelError::UnknownThreatScenario(id))?;
            let (base_f, step_tips) = self.entry_tipping_points(entry, residual)?;
            let base_r = self.max_risk_for_feasibility(t, base_f);
            if let Some(from_risk) = base_r {
                for st in step_tips {
                    if let Some(to_risk) = self.max_risk_for_feasibility(t, st.to_root)
                        && to_risk != from_risk
                    {
                        feasibility_tipping_points.push(FeasibilityTippingPoint {
                            step: Some(st.step),
                            node: Some(st.node),
                            from_step_feasibility: st.from_step,
                            to_step_feasibility: st.to_step,
                            from_threat_feasibility: st.from_root,
                            to_threat_feasibility: st.to_root,
                            from_risk,
                            to_risk,
                        });
                    }
                }
            }
            (base_f, base_r)
        } else {
            let base_f = t.direct_feasibility.ok_or(ModelError::NoAttackTree(id))?;
            let base_r = self.max_risk_for_feasibility(t, base_f);
            if let Some(from_risk) = base_r {
                let neighbors = adjacent_feasibility(base_f, feas_count);
                for to_f in neighbors.into_iter().flatten() {
                    if let Some(to_risk) = self.max_risk_for_feasibility(t, to_f)
                        && to_risk != from_risk
                    {
                        feasibility_tipping_points.push(FeasibilityTippingPoint {
                            step: None,
                            node: None,
                            from_step_feasibility: base_f,
                            to_step_feasibility: to_f,
                            from_threat_feasibility: base_f,
                            to_threat_feasibility: to_f,
                            from_risk,
                            to_risk,
                        });
                    }
                }
            }
            (base_f, base_r)
        };

        let impact_tipping_points = baseline_risk
            .map(|from_risk| self.impact_tipping_points_for(t, baseline_feasibility, from_risk))
            .unwrap_or_default();

        Ok(ThreatSensitivity {
            feasibility: baseline_feasibility,
            max_risk: baseline_risk,
            feasibility_tipping_points,
            impact_tipping_points,
        })
    }

    fn impact_tipping_points_for(
        &self,
        t: &ThreatScenario,
        baseline_feasibility: FeasibilityLevel,
        from_risk: RiskLevel,
    ) -> Vec<ImpactTippingPoint> {
        let mut rated_entries = Vec::new();
        let mut max1: Option<RiskLevel> = None;
        let mut max1_count = 0_usize;
        let mut max2: Option<RiskLevel> = None;
        for &damage_idx in &t.damages {
            let (Some(damage_id), Some(damage)) =
                (self.damages.id(damage_idx), self.damages.get(damage_idx))
            else {
                continue;
            };
            for (category, impact) in damage.impacts() {
                let Some(from_impact) = impact else { continue };
                let Some(risk) =
                    self.methodology
                        .risk_for(category, from_impact, baseline_feasibility)
                else {
                    continue;
                };
                match max1 {
                    None => {
                        max1 = Some(risk);
                        max1_count = 1;
                    }
                    Some(m1) if risk > m1 => {
                        max2 = Some(m1);
                        max1 = Some(risk);
                        max1_count = 1;
                    }
                    Some(m1) if risk == m1 => {
                        max1_count += 1;
                    }
                    Some(_) => {
                        if max2.is_none_or(|m2| risk > m2) {
                            max2 = Some(risk);
                        }
                    }
                }
                rated_entries.push((damage_id, category, from_impact, risk));
            }
        }

        let mut out = Vec::new();
        for (damage_id, category, from_impact, entry_risk) in rated_entries {
            let other_max = if Some(entry_risk) == max1 && max1_count == 1 {
                max2
            } else {
                max1
            };
            let cat_count = self.methodology.level_count(category);
            let neighbors = adjacent_impact(from_impact, cat_count);
            for to_impact in neighbors.into_iter().flatten() {
                let Some(new_entry_risk) =
                    self.methodology
                        .risk_for(category, to_impact, baseline_feasibility)
                else {
                    continue;
                };
                let to_risk = other_max.map_or(new_entry_risk, |other| other.max(new_entry_risk));
                if to_risk != from_risk {
                    out.push(ImpactTippingPoint {
                        damage: damage_id,
                        category,
                        from_impact,
                        to_impact,
                        from_risk,
                        to_risk,
                    });
                }
            }
        }
        out
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

    /// Recomputes `risk.entries` and `risk.deciding` in place from `t` and
    /// `risk.feasibility`, reusing the existing `Vec` capacity and leaving
    /// `risk.tree_evaluation` intact. Time O(d * c), space O(1) amortized.
    fn update_risk_entries(&self, t: &ThreatScenario, risk: &mut ThreatRisk) {
        let feasibility = risk.feasibility;
        risk.entries.clear();
        let mut deciding: Option<usize> = None;
        for &damage_idx in &t.damages {
            let (Some(damage_id), Some(damage)) =
                (self.damages.id(damage_idx), self.damages.get(damage_idx))
            else {
                continue;
            };
            for (category, impact) in damage.impacts() {
                let Some(impact) = impact else { continue };
                let Some(level) = self.methodology.risk_for(category, impact, feasibility) else {
                    continue;
                };
                let is_higher = deciding
                    .and_then(|idx| risk.entries.get(idx))
                    .is_none_or(|best: &ImpactRisk| level > best.risk);
                if is_higher {
                    deciding = Some(risk.entries.len());
                }
                risk.entries.push(ImpactRisk {
                    damage: damage_id,
                    category,
                    impact,
                    feasibility,
                    risk: level,
                });
            }
        }
        risk.deciding = deciding;
    }

    /// Builds a dependency-tracked incremental evaluation cache over this
    /// project (rules A1, D10).
    ///
    /// Equivalent to [`ProjectEvalCache::new(self)`](ProjectEvalCache::new).
    /// Time O(K * (V + E) + T * (V + d * c)), space O(K * V + T * (V + d * c)).
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
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_risk(&project, threat)?, &project.evaluate_risk(threat)?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn eval_cache(&self) -> ProjectEvalCache {
        ProjectEvalCache::new(self)
    }

    /// Validates and applies `op` to `self`, incrementally updating `cache`
    /// on success and returning the inverse [`Op`] (rules A1, D10).
    ///
    /// If `self.apply(op)` returns an error, neither `self` nor `cache` is
    /// modified.
    ///
    /// Time O(cost of `apply(op)` + incremental recomputation of affected
    /// tree nodes and dependent threat risks).
    ///
    /// # Errors
    ///
    /// Same as [`apply`](Self::apply).
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
    /// let (low, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
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
    /// let mut cache = project.eval_cache();
    /// project.apply_cached(
    ///     Op::SetStepFeasibility {
    ///         tree: tree_id, node: step, level: Some(low), provenance: Provenance::default(),
    ///     },
    ///     &mut cache,
    /// )?;
    /// assert_eq!(cache.evaluate_risk(&project, threat)?.max_risk(), m.risk_level("r3"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn apply_cached(&mut self, op: Op, cache: &mut ProjectEvalCache) -> Result<Op, ModelError> {
        let inv = self.apply(op)?;
        cache.update(self, &inv);
        Ok(inv)
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
        self.lint_setting_conflicts(&mut findings);
        self.lint_goals(&mut findings);
        self.lint_claims(&mut findings);
        self.lint_data_flows(&mut findings);
        self.lint_components(&mut findings);
        self.lint_systems(&mut findings);
        findings.sort_unstable();
        findings
    }

    fn lint_assets(&self, findings: &mut Vec<LintFinding>) {
        let has_components = self.components.len() > 0;
        for (idx, id, asset) in self.assets.iter() {
            if has_components && asset.component().is_none() {
                findings.push(LintFinding::new(LintCode::AssetUnbound { asset: id }));
            }
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
        let rationale = self.methodology.requires_rationale();
        for (idx, id, damage) in self.damages.iter() {
            if damage.impacts().all(|(_, level)| level.is_none()) {
                findings.push(LintFinding::new(LintCode::DamageScenarioWithoutImpacts {
                    damage: id,
                }));
            } else if rationale && damage.impact_rationale.trim().is_empty() {
                findings.push(LintFinding::new(LintCode::ImpactWithoutRationale {
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
        let rationale = self.methodology.requires_rationale();
        for (_idx, id, threat) in self.threats.iter() {
            if rationale
                && threat.direct_feasibility.is_some()
                && threat.feasibility_rationale.trim().is_empty()
            {
                findings.push(LintFinding::new(LintCode::FeasibilityWithoutRationale {
                    threat: id,
                }));
            }
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
                        .is_some_and(|entry| {
                            entry.step_mitigations.iter().any(|m| !m.is_empty())
                                || (0..entry.tree.len())
                                    .any(|i| self.entry_step_has_rule_ceiling(entry, i))
                        });
                    if !has_mitigations {
                        findings.push(LintFinding::new(LintCode::ReduceTreatmentWithoutControls {
                            threat: id,
                        }));
                    }
                    if threat.goal().is_none() {
                        findings.push(LintFinding::new(LintCode::ReduceTreatmentWithoutGoal {
                            threat: id,
                        }));
                    }
                }
                Some(RiskTreatment::Share | RiskTreatment::Retain) => {
                    if let Some(claim_idx) = threat.claim() {
                        if threat.treatment() == Some(RiskTreatment::Share) {
                            self.lint_share_party(id, claim_idx, findings);
                        }
                    } else {
                        findings.push(LintFinding::new(
                            LintCode::RetainOrShareTreatmentWithoutClaim { threat: id },
                        ));
                    }
                }
                Some(RiskTreatment::Avoid) => {}
            }
            if let Some(goal) = threat
                .goal()
                .filter(|_| threat.treatment() != Some(RiskTreatment::Reduce))
                .and_then(|goal_idx| self.goals.id(goal_idx))
            {
                findings.push(LintFinding::new(LintCode::GoalOnNonReduceThreat {
                    threat: id,
                    goal,
                }));
            }
            if let Some(claim) = threat
                .claim()
                .filter(|_| {
                    !matches!(
                        threat.treatment(),
                        Some(RiskTreatment::Retain | RiskTreatment::Share)
                    )
                })
                .and_then(|claim_idx| self.claims.id(claim_idx))
            {
                findings.push(LintFinding::new(LintCode::ClaimOnNonRetainOrShareThreat {
                    threat: id,
                    claim,
                }));
            }
        }
    }

    /// A `Share` threat whose claim names no party other than `Us` warns.
    fn lint_share_party(
        &self,
        threat: ThreatId,
        claim_idx: ClaimIdx,
        findings: &mut Vec<LintFinding>,
    ) {
        let has_other = self.claims.get(claim_idx).is_some_and(|claim| {
            claim.responsible().iter().any(|&party_idx| {
                self.party(party_idx)
                    .is_some_and(|party| party.kind() != PartyKind::Us)
            })
        });
        if !has_other {
            findings.push(LintFinding::new(
                LintCode::ShareTreatmentWithoutOtherParty { threat },
            ));
        }
    }

    fn lint_trees(&self, findings: &mut Vec<LintFinding>) {
        for &(tree, node) in &self.review_marks {
            findings.push(LintFinding::new(LintCode::LibraryRatingNotReviewed {
                tree,
                node,
            }));
        }
        let factor_count = self.methodology.feasibility_factor_count();
        let any_rules = self.controls.iter().any(|(_, _, c)| !c.rules().is_empty());
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
                            .effective_step_feasibility(&self.methodology, node_idx)
                            .is_none()
                        {
                            findings.push(LintFinding::new(LintCode::AttackTreeStepUnrated {
                                tree: id,
                                node: node_id,
                            }));
                        }
                        if any_rules
                            && entry.via_at(node_idx.index()).is_some()
                                != entry.attack_type_at(node_idx.index()).is_some()
                        {
                            findings.push(LintFinding::new(LintCode::StepSettingsIncomplete {
                                tree: id,
                                node: node_id,
                            }));
                        }
                        let rated_factors = entry
                            .step_factors
                            .get(node_idx.index())
                            .map_or(0, |r| r.iter().filter(|l| l.is_some()).count());
                        if rated_factors > 0 && rated_factors < factor_count {
                            findings.push(LintFinding::new(
                                LintCode::AttackTreeStepIncompleteFactors {
                                    tree: id,
                                    node: node_id,
                                },
                            ));
                        } else if rated_factors == factor_count
                            && factor_count > 0
                            && entry
                                .step_feasibility
                                .get(node_idx.index())
                                .copied()
                                .flatten()
                                .is_some()
                        {
                            findings.push(LintFinding::new(LintCode::RedundantStepFeasibility {
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
        for (idx, id, control) in self.controls.iter() {
            if self.refs.controls.get(idx.index()).copied().unwrap_or(0) == 0
                && control.targets.is_empty()
            {
                findings.push(LintFinding::new(LintCode::SecurityControlUnused {
                    control: id,
                }));
            }
            if self.control_differs_from_kind(idx) {
                findings.push(LintFinding::new(LintCode::ControlRulesDifferFromKind {
                    control: id,
                }));
            }
        }
    }

    /// Lints conflicting setting presets applied to the same target (CFG-09).
    /// Time O(C * T + K * F), C live controls with T <= 64 targets each,
    /// F <= 64 declared conflicts per preset; space O(K + P), P pairs found.
    fn lint_setting_conflicts(&self, findings: &mut Vec<LintFinding>) {
        // Early-return when no preset declares any conflict.
        let any_conflict = self
            .methodology
            .control_kinds()
            .any(|k| !self.methodology.control_kind_conflicts(k).is_empty());
        if !any_conflict {
            return;
        }
        // Build per-target set of kinds.
        let mut by_target: BTreeMap<ItemIdx, BTreeSet<ControlKind>> = BTreeMap::new();
        for (_idx, _id, control) in self.controls.iter() {
            if let Some(kind) = control.kind {
                for &t in &control.targets {
                    by_target.entry(t).or_default().insert(kind);
                }
            }
        }
        for (target, kinds) in &by_target {
            let mut pairs: BTreeSet<(ControlKind, ControlKind)> = BTreeSet::new();
            for &k in kinds {
                for &other in self.methodology.control_kind_conflicts(k) {
                    if kinds.contains(&other) {
                        let (a, b) = if k.index() < other.index() {
                            (k, other)
                        } else {
                            (other, k)
                        };
                        pairs.insert((a, b));
                    }
                }
            }
            for (first, second) in pairs {
                let Some(item) = self.item_ref(Some(*target)) else {
                    continue;
                };
                findings.push(LintFinding::new(LintCode::ConflictingSettings {
                    item,
                    first,
                    second,
                }));
            }
        }
    }

    fn lint_goals(&self, findings: &mut Vec<LintFinding>) {
        for (idx, id, goal) in self.goals.iter() {
            if goal.controls().is_empty() {
                findings.push(LintFinding::new(
                    LintCode::CybersecurityGoalWithoutControls { goal: id },
                ));
            }
            if self.refs.goals.get(idx.index()).copied().unwrap_or(0) == 0 {
                findings.push(LintFinding::new(LintCode::CybersecurityGoalUnlinked {
                    goal: id,
                }));
            }
        }
    }

    fn lint_claims(&self, findings: &mut Vec<LintFinding>) {
        for (idx, id, _claim) in self.claims.iter() {
            if self.refs.claims.get(idx.index()).copied().unwrap_or(0) == 0 {
                findings.push(LintFinding::new(LintCode::CybersecurityClaimUnlinked {
                    claim: id,
                }));
            }
        }
    }

    fn lint_components(&self, findings: &mut Vec<LintFinding>) {
        if self.element_count(ElementKind::System) == 0 {
            return;
        }
        for (_idx, id, component) in self.components.iter() {
            if component.parent().is_none() && component.system().is_none() {
                findings.push(LintFinding::new(LintCode::ComponentWithoutSystem {
                    component: id,
                }));
            }
        }
    }

    fn lint_systems(&self, findings: &mut Vec<LintFinding>) {
        let mut first_by_name: BTreeMap<&str, ElementId> = BTreeMap::new();
        for (id, system) in self.elements(ElementKind::System) {
            if let Some(&first) = first_by_name.get(system.name()) {
                findings.push(LintFinding::new(LintCode::SystemNameShared {
                    system: id,
                    first,
                }));
            } else {
                first_by_name.insert(system.name(), id);
            }
        }
    }

    fn lint_data_flows(&self, findings: &mut Vec<LintFinding>) {
        let has_systems = self.element_count(ElementKind::System) > 0;
        if self.data_flows.len() == 0 || (self.trust_boundaries.len() == 0 && !has_systems) {
            return;
        }
        let mut comp_has_threat = vec![false; self.components.slot_count()];
        for (a_idx, _id, asset) in self.assets.iter() {
            if let Some(comp_idx) = asset.component()
                && self.refs.assets.get(a_idx.index()).copied().unwrap_or(0) > 0
                && let Some(slot) = comp_has_threat.get_mut(comp_idx.index())
            {
                *slot = true;
            }
        }
        for (idx, id, flow) in self.data_flows.iter() {
            let crosses_boundary = self
                .trust_boundaries
                .iter()
                .any(|(_, _, tb)| tb.contains(flow.from()) != tb.contains(flow.to()));
            let between_systems = has_systems && self.boundary_flow(idx).is_some();
            if crosses_boundary || between_systems {
                let from_threat = comp_has_threat
                    .get(flow.from().index())
                    .copied()
                    .unwrap_or(false);
                let to_threat = comp_has_threat
                    .get(flow.to().index())
                    .copied()
                    .unwrap_or(false);
                if !from_threat && !to_threat {
                    findings.push(LintFinding::new(if crosses_boundary {
                        LintCode::BoundaryCrossingDataFlowWithoutThreat { flow: id }
                    } else {
                        LintCode::SystemBoundaryFlowWithoutThreat { flow: id }
                    }));
                }
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
        let assets = diff_arena(&self.assets, &other.assets, |x, y| {
            self.same_asset(x, other, y)
        });
        let damage_scenarios = diff_arena(&self.damages, &other.damages, |x, y| x == y);
        let security_controls = diff_arena(&self.controls, &other.controls, |x, y| {
            self.same_control(x, other, y)
        });
        let cybersecurity_goals = diff_arena(&self.goals, &other.goals, |x, y| {
            self.same_goal(x, other, y)
        });
        let cybersecurity_claims = diff_arena(&self.claims, &other.claims, |x, y| {
            self.same_claim(x, other, y)
        });
        let parties = diff_arena(&self.parties, &other.parties, |x, y| x == y);
        let components = diff_arena(&self.components, &other.components, |x, y| {
            self.same_component(x, other, y)
        });
        let interfaces = diff_arena(&self.interfaces, &other.interfaces, |x, y| {
            self.same_interface(x, other, y)
        });
        let data_flows = diff_arena(&self.data_flows, &other.data_flows, |x, y| {
            self.same_data_flow(x, other, y)
        });
        let trust_boundaries =
            diff_arena(&self.trust_boundaries, &other.trust_boundaries, |x, y| {
                self.same_trust_boundary(x, other, y)
            });
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
            &cybersecurity_goals,
            &cybersecurity_claims,
        );
        // A changed description changes no number, so it is merged after the
        // risk deltas: it marks the element modified and nothing more.
        let mut diff = ProjectDiff {
            assets,
            damage_scenarios,
            threat_scenarios,
            attack_trees,
            security_controls,
            cybersecurity_goals,
            cybersecurity_claims,
            parties,
            elements: self.elements.diff(&other.elements),
            components,
            interfaces,
            data_flows,
            trust_boundaries,
            risk_deltas,
            notes_changed: false,
            item_text_changed: false,
            languages_changed: self.languages() != other.languages(),
        };
        self.merge_description_changes(other, &mut diff);
        Ok(diff)
    }

    /// Marks elements whose description differs as modified, and sets the
    /// notes flag. Time O(d log d), d = descriptions in both projects.
    fn merge_description_changes(&self, other: &Self, diff: &mut ProjectDiff) {
        let changed = self
            .descriptions
            .keys()
            .chain(other.descriptions.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .filter(|t| self.description(*t) != other.description(*t));
        let (mut assets, mut damages, mut threats, mut trees) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let (mut controls, mut goals, mut claims) = (Vec::new(), Vec::new(), Vec::new());
        let (mut components, mut interfaces, mut data_flows, mut trust_boundaries) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for target in changed {
            match target {
                ElementRef::Project => diff.notes_changed = true,
                ElementRef::ItemDescription | ElementRef::ItemScope => {
                    diff.item_text_changed = true;
                }
                ElementRef::Asset(id) => assets.push(id),
                ElementRef::Damage(id) => damages.push(id),
                ElementRef::Threat(id) => threats.push(id),
                ElementRef::Tree(id) => trees.push(id),
                ElementRef::Control(id) => controls.push(id),
                ElementRef::Goal(id) => goals.push(id),
                ElementRef::Claim(id) => claims.push(id),
                ElementRef::Component(id) => components.push(id),
                ElementRef::Interface(id) => interfaces.push(id),
                ElementRef::DataFlow(id) => data_flows.push(id),
                ElementRef::TrustBoundary(id) => trust_boundaries.push(id),
            }
        }
        merge_modified(&mut diff.assets, assets);
        merge_modified(&mut diff.damage_scenarios, damages);
        merge_modified(&mut diff.threat_scenarios, threats);
        merge_modified(&mut diff.attack_trees, trees);
        merge_modified(&mut diff.security_controls, controls);
        merge_modified(&mut diff.cybersecurity_goals, goals);
        merge_modified(&mut diff.cybersecurity_claims, claims);
        merge_modified(&mut diff.components, components);
        merge_modified(&mut diff.interfaces, interfaces);
        merge_modified(&mut diff.data_flows, data_flows);
        merge_modified(&mut diff.trust_boundaries, trust_boundaries);
    }

    #[allow(clippy::too_many_arguments)]
    fn diff_risk_deltas(
        &self,
        other: &Self,
        assets: &EntityDiff<AssetId>,
        damages: &EntityDiff<DamageId>,
        threats: &EntityDiff<ThreatId>,
        trees: &EntityDiff<TreeId>,
        controls: &EntityDiff<ControlId>,
        goals: &EntityDiff<GoalId>,
        claims: &EntityDiff<ClaimId>,
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
                            other, id_a, t_a, assets, damages, threats, trees, controls, goals,
                            claims,
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
        other: &Self,
        id: ThreatId,
        t: &ThreatScenario,
        assets: &EntityDiff<AssetId>,
        damages: &EntityDiff<DamageId>,
        threats: &EntityDiff<ThreatId>,
        trees: &EntityDiff<TreeId>,
        controls: &EntityDiff<ControlId>,
        goals: &EntityDiff<GoalId>,
        claims: &EntityDiff<ClaimId>,
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
            if !controls.is_empty() && self.tree_affected_by_controls(other, tree_idx, controls) {
                return true;
            }
        }
        if let Some(goal_idx) = t.goal {
            if self
                .goals
                .id(goal_idx)
                .is_some_and(|g| goals.modified().binary_search(&g).is_ok())
            {
                return true;
            }
            if !controls.modified().is_empty()
                && self.goals.get(goal_idx).is_some_and(|goal| {
                    goal.controls
                        .iter()
                        .filter_map(|&c| self.controls.id(c))
                        .any(|c| controls.modified().binary_search(&c).is_ok())
                })
            {
                return true;
            }
        }
        if t.claim
            .and_then(|claim_idx| self.claims.id(claim_idx))
            .is_some_and(|cl| claims.modified().binary_search(&cl).is_ok())
        {
            return true;
        }
        false
    }

    fn tree_affected_by_controls(
        &self,
        other: &Self,
        tree_idx: TreeIdx,
        controls: &EntityDiff<ControlId>,
    ) -> bool {
        let Some(entry) = self.trees.get(tree_idx) else {
            return false;
        };
        if !controls.modified().is_empty()
            && entry
                .step_mitigations
                .iter()
                .flat_map(|ms| ms.iter())
                .filter_map(|m| self.controls.id(m.control))
                .any(|c| controls.modified().binary_search(&c).is_ok())
        {
            return true;
        }
        for (_, via) in entry.vias() {
            if self.item_affected_by_controls(via, &[controls.removed(), controls.modified()]) {
                return true;
            }
            if let Some(via_ref) = self.item_ref(Some(via))
                && let Ok(other_via) = other.item_idx(via_ref)
                && other
                    .item_affected_by_controls(other_via, &[controls.added(), controls.modified()])
            {
                return true;
            }
        }
        false
    }

    fn item_affected_by_controls(&self, via: ItemIdx, lists: &[&[ControlId]]) -> bool {
        let owner = self.item_owner_component(via);
        if self.item_control_count(via) == 0
            && owner.is_none_or(|o| self.item_control_count(o) == 0)
        {
            return false;
        }
        self.controls.iter().any(|(_, id, ctrl)| {
            (ctrl.targets.contains(&via) || owner.is_some_and(|o| ctrl.targets.contains(&o)))
                && lists.iter().any(|list| list.binary_search(&id).is_ok())
        })
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
            Op::SetDescription {
                target,
                text,
                provenance,
            } => return self.check_set_description(*target, text, provenance),
            Op::SetDescriptionTranslation {
                target,
                lang,
                text,
                provenance,
            } => return self.check_set_description_translation(*target, lang, text, provenance),
            Op::SetProjectLanguages { langs, provenance } => {
                if langs.len() > MAX_DESCRIPTION_TRANSLATIONS {
                    return Err(ModelError::TooManyLanguages {
                        max: MAX_DESCRIPTION_TRANSLATIONS,
                    });
                }
                for (i, lang) in langs.iter().enumerate() {
                    check_lang(lang)?;
                    if langs[..i].contains(lang) {
                        return Err(ModelError::DuplicateLanguage);
                    }
                }
                check_provenance(provenance)?;
            }
            Op::SetReviewMark {
                tree,
                node,
                provenance,
                ..
            } => {
                let (entry, idx) = self.tree_node(*tree, *node)?;
                if !matches!(entry.tree.get(idx), Some(Node::Step { .. })) {
                    return Err(ModelError::NotAStep {
                        tree: *tree,
                        node: *node,
                    });
                }
                check_provenance(provenance)?;
            }
            Op::AddTreeNode {
                tree,
                parent,
                id,
                node,
                index,
                position,
                provenance,
            } => {
                return self
                    .check_add_tree_node(*tree, *parent, *id, node, *index, *position, provenance);
            }
            Op::RemoveTreeNode {
                tree,
                id,
                provenance,
            } => return self.check_remove_tree_node(*tree, *id, provenance),
            Op::LinkTreeNode {
                tree,
                parent,
                child,
                position,
                slot,
                provenance,
            } => {
                return self
                    .check_link_tree_node(*tree, *parent, *child, *position, *slot, provenance);
            }
            Op::UnlinkTreeNode {
                tree,
                parent,
                child,
                provenance,
            } => return self.check_unlink_tree_node(*tree, *parent, *child, provenance),
            Op::RenameTreeNode {
                tree,
                id,
                label,
                provenance,
            } => {
                let (_, idx) = self.tree_node(*tree, *id)?;
                let _ = idx;
                check_name(EntityKind::TreeNode, label)?;
                check_provenance(provenance)?;
            }
            Op::SetTreeNodeGate {
                tree,
                id,
                provenance,
                ..
            } => {
                let (entry, idx) = self.tree_node(*tree, *id)?;
                if !matches!(entry.tree.get(idx), Some(Node::Gate { .. })) {
                    return Err(ModelError::NotAGate(idx));
                }
                check_provenance(provenance)?;
            }
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
            Op::SetStepVia {
                tree, node, via, ..
            } => self.check_set_step_via(*tree, *node, *via)?,
            Op::SetStepAttackType {
                tree,
                node,
                attack_type,
                ..
            } => self.check_set_step_attack_type(*tree, *node, *attack_type)?,
            Op::SetStepFactor {
                tree,
                node,
                factor,
                level,
                ..
            } => self.check_set_step_factor(*tree, *node, *factor, *level)?,
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
            Op::ApplyControl {
                control, target, ..
            } => self.check_control_target(*control, *target, true)?,
            Op::UnapplyControl {
                control, target, ..
            } => self.check_control_target(*control, *target, false)?,
            Op::SetControlRule {
                control,
                attack_type,
                ceiling,
                ..
            } => self.check_set_control_rule(*control, *attack_type, *ceiling)?,
            Op::SetControlKind { control, kind, .. } => {
                self.check_set_control_kind(*control, *kind)?;
            }
            Op::SetStepMitigation {
                tree,
                node,
                control,
                feasibility,
                ..
            } => self.check_set_step_mitigation(*tree, *node, *control, *feasibility)?,
            Op::AddCybersecurityGoal {
                id,
                name,
                controls,
                provenance,
            } => return self.check_add_goal(*id, name, controls, provenance),
            Op::RenameCybersecurityGoal { id, name, .. } => {
                self.live_goal(*id)?;
                check_name(EntityKind::CybersecurityGoal, name)?;
            }
            Op::LinkGoalControl { goal, control, .. } => {
                self.check_goal_control(*goal, *control, true)?;
            }
            Op::UnlinkGoalControl { goal, control, .. } => {
                self.check_goal_control(*goal, *control, false)?;
            }
            Op::LinkGoalParty { goal, party, .. } => self.check_goal_party(*goal, *party, true)?,
            Op::UnlinkGoalParty { goal, party, .. } => {
                self.check_goal_party(*goal, *party, false)?;
            }
            Op::RemoveCybersecurityGoal { id } => return self.check_remove_goal(*id),
            Op::AddCybersecurityClaim {
                id,
                name,
                provenance,
            } => return self.check_add_claim(*id, name, provenance),
            Op::RenameCybersecurityClaim { id, name, .. } => {
                self.live_claim(*id)?;
                check_name(EntityKind::CybersecurityClaim, name)?;
            }
            Op::LinkClaimParty { claim, party, .. } => {
                self.check_claim_party(*claim, *party, true)?;
            }
            Op::UnlinkClaimParty { claim, party, .. } => {
                self.check_claim_party(*claim, *party, false)?;
            }
            Op::RemoveCybersecurityClaim { id } => return self.check_remove_claim(*id),
            Op::AddParty {
                id,
                name,
                provenance,
                ..
            } => return self.check_add_party(*id, name, provenance),
            Op::RenameParty { id, name, .. } => {
                self.party_index(*id)?;
                check_party_name(name)?;
            }
            Op::SetPartyKind { id, .. } => {
                self.party_index(*id)?;
            }
            Op::RemoveParty { id } => return self.check_remove_party(*id),
            Op::AddElement { .. }
            | Op::RenameElement { .. }
            | Op::RemoveElement { .. }
            | Op::SetElementAttribute { .. }
            | Op::LinkElement { .. }
            | Op::UnlinkElement { .. } => {
                self.check_element_op(op).unwrap_or(Ok(()))?;
            }
            Op::SetThreatGoal { threat, goal, .. } => {
                self.check_set_threat_goal(*threat, *goal)?;
            }
            Op::SetThreatClaim { threat, claim, .. } => {
                self.check_set_threat_claim(*threat, *claim)?;
            }
            Op::SetThreatScaling { threat, reason, .. } => {
                self.live_threat(*threat)?;
                check_text(reason, MAX_DESCRIPTION_CHARS)?;
            }
            Op::SetImpactRationale { damage, text, .. } => {
                self.live_damage(*damage)?;
                check_text(text, MAX_DESCRIPTION_CHARS)?;
            }
            Op::SetFeasibilityRationale { threat, text, .. } => {
                self.live_threat(*threat)?;
                check_text(text, MAX_DESCRIPTION_CHARS)?;
            }
            Op::SetDamageLinkRationale {
                threat,
                damage,
                text,
                ..
            } => {
                self.check_link(*threat, *damage, false)?;
                check_text(text, MAX_DESCRIPTION_CHARS)?;
            }
            Op::AddComponent {
                id,
                name,
                parent,
                provenance,
            } => return self.check_add_component(*id, name, *parent, provenance),
            Op::RenameComponent { id, name, .. } => {
                self.live_component(*id)?;
                check_name(EntityKind::Component, name)?;
            }
            Op::SetComponentParent { id, parent, .. } => {
                self.check_set_component_parent(*id, *parent)?;
            }
            Op::SetComponentSystem {
                id,
                system,
                provenance,
            } => return self.check_set_component_system(*id, *system, provenance),
            Op::RemoveComponent { id } => return self.check_remove_component(*id),
            Op::AddInterface {
                id,
                name,
                component,
                kind,
                provenance,
            } => return self.check_add_interface(*id, name, *component, *kind, provenance),
            Op::RenameInterface { id, name, .. } => {
                self.live_interface(*id)?;
                check_name(EntityKind::Interface, name)?;
            }
            Op::SetInterfaceKind { id, kind, .. } => {
                self.live_interface(*id)?;
                self.methodology.check_interface_kind(*kind)?;
            }
            Op::RemoveInterface { id } => return self.check_remove_interface(*id),
            Op::AddDataFlow {
                id,
                name,
                from,
                to,
                interface,
                provenance,
            } => {
                return self.check_add_data_flow(*id, name, *from, *to, *interface, provenance);
            }
            Op::RenameDataFlow { id, name, .. } => {
                self.live_data_flow(*id)?;
                check_name(EntityKind::DataFlow, name)?;
            }
            Op::SetDataFlowInterface { id, interface, .. } => {
                self.check_set_data_flow_interface(*id, *interface)?;
            }
            Op::RemoveDataFlow { id } => return self.check_remove_data_flow(*id),
            Op::AddTrustBoundary {
                id,
                name,
                components,
                provenance,
            } => return self.check_add_trust_boundary(*id, name, components, provenance),
            Op::RenameTrustBoundary { id, name, .. } => {
                self.live_trust_boundary(*id)?;
                check_name(EntityKind::TrustBoundary, name)?;
            }
            Op::AddBoundaryComponent {
                boundary,
                component,
                ..
            } => {
                self.check_boundary_component(*boundary, *component, true)?;
            }
            Op::RemoveBoundaryComponent {
                boundary,
                component,
                ..
            } => {
                self.check_boundary_component(*boundary, *component, false)?;
            }
            Op::RemoveTrustBoundary { id } => return self.check_remove_trust_boundary(*id),
            Op::SetAssetComponent {
                asset, component, ..
            } => {
                self.check_set_asset_component(*asset, *component)?;
            }
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
            Op::SetDescription {
                target,
                text,
                provenance,
            } => Ok(self.commit_set_description(target, text, provenance)),
            Op::SetDescriptionTranslation {
                target,
                lang,
                text,
                provenance,
            } => Ok(self.commit_set_description_translation(target, lang, text, provenance)),
            Op::SetProjectLanguages { langs, provenance } => {
                let (langs, provenance) =
                    std::mem::replace(&mut self.languages, (langs, provenance));
                Ok(Op::SetProjectLanguages { langs, provenance })
            }
            Op::SetReviewMark {
                tree,
                node,
                marked,
                provenance,
            } => {
                let entry = self.tree_mut(tree)?;
                let provenance = std::mem::replace(&mut entry.provenance, provenance);
                let was = if marked {
                    !self.review_marks.insert((tree, node))
                } else {
                    self.review_marks.remove(&(tree, node))
                };
                Ok(Op::SetReviewMark {
                    tree,
                    node,
                    marked: was,
                    provenance,
                })
            }
            Op::AddTreeNode {
                tree,
                parent,
                id,
                node,
                index,
                position,
                provenance,
            } => self.commit_add_tree_node(tree, parent, id, node, index, position, provenance),
            Op::RemoveTreeNode {
                tree,
                id,
                provenance,
            } => self.commit_remove_tree_node(tree, id, provenance),
            Op::LinkTreeNode {
                tree,
                parent,
                child,
                position,
                slot,
                provenance,
            } => self.commit_link_tree_node(tree, parent, child, position, slot, provenance),
            Op::UnlinkTreeNode {
                tree,
                parent,
                child,
                provenance,
            } => self.commit_unlink_tree_node(tree, parent, child, provenance),
            Op::RenameTreeNode {
                tree,
                id,
                label,
                provenance,
            } => {
                let entry = self.tree_mut(tree)?;
                let idx = entry
                    .tree
                    .idx_of(&id)
                    .ok_or(ModelError::UnknownTreeNode { tree, node: id })?;
                let label = entry.tree.set_label(idx, label)?;
                let provenance = std::mem::replace(&mut entry.provenance, provenance);
                Ok(Op::RenameTreeNode {
                    tree,
                    id,
                    label,
                    provenance,
                })
            }
            Op::SetTreeNodeGate {
                tree,
                id,
                gate,
                provenance,
            } => {
                let entry = self.tree_mut(tree)?;
                let idx = entry
                    .tree
                    .idx_of(&id)
                    .ok_or(ModelError::UnknownTreeNode { tree, node: id })?;
                let gate = entry.tree.set_gate_kind(idx, gate)?;
                let provenance = std::mem::replace(&mut entry.provenance, provenance);
                Ok(Op::SetTreeNodeGate {
                    tree,
                    id,
                    gate,
                    provenance,
                })
            }
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
            Op::SetStepVia {
                tree,
                node,
                via,
                provenance,
            } => self.commit_set_step_via(tree, node, via, provenance),
            Op::SetStepAttackType {
                tree,
                node,
                attack_type,
                provenance,
            } => self.commit_set_step_attack_type(tree, node, attack_type, provenance),
            Op::SetStepFactor {
                tree,
                node,
                factor,
                level,
                provenance,
            } => self.commit_set_step_factor(tree, node, factor, level, provenance),
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
            Op::ApplyControl {
                control,
                target,
                provenance,
            } => self.commit_control_target(control, target, provenance, true),
            Op::UnapplyControl {
                control,
                target,
                provenance,
            } => self.commit_control_target(control, target, provenance, false),
            Op::SetControlRule {
                control,
                attack_type,
                ceiling,
                provenance,
            } => self.commit_set_control_rule(control, attack_type, ceiling, provenance),
            Op::SetControlKind {
                control,
                kind,
                provenance,
            } => self.commit_set_control_kind(control, kind, provenance),
            Op::SetStepMitigation {
                tree,
                node,
                control,
                feasibility,
                provenance,
            } => self.commit_set_step_mitigation(tree, node, control, feasibility, provenance),
            Op::AddCybersecurityGoal {
                id,
                name,
                controls,
                provenance,
            } => self.commit_add_goal(id, name, controls, provenance),
            Op::RenameCybersecurityGoal {
                id,
                name,
                provenance,
            } => self.commit_rename_goal(id, name, provenance),
            Op::LinkGoalControl {
                goal,
                control,
                provenance,
            } => self.commit_goal_control(goal, control, provenance, true),
            Op::UnlinkGoalControl {
                goal,
                control,
                provenance,
            } => self.commit_goal_control(goal, control, provenance, false),
            Op::LinkGoalParty {
                goal,
                party,
                provenance,
            } => self.commit_goal_party(goal, party, provenance, true),
            Op::UnlinkGoalParty {
                goal,
                party,
                provenance,
            } => self.commit_goal_party(goal, party, provenance, false),
            Op::RemoveCybersecurityGoal { id } => self.commit_remove_goal(id),
            Op::AddCybersecurityClaim {
                id,
                name,
                provenance,
            } => self.commit_add_claim(id, name, provenance),
            Op::RenameCybersecurityClaim {
                id,
                name,
                provenance,
            } => self.commit_rename_claim(id, name, provenance),
            Op::LinkClaimParty {
                claim,
                party,
                provenance,
            } => self.commit_claim_party(claim, party, provenance, true),
            Op::UnlinkClaimParty {
                claim,
                party,
                provenance,
            } => self.commit_claim_party(claim, party, provenance, false),
            Op::RemoveCybersecurityClaim { id } => self.commit_remove_claim(id),
            Op::AddParty {
                id,
                name,
                kind,
                provenance,
            } => self.commit_add_party(id, name, kind, provenance),
            Op::RenameParty {
                id,
                name,
                provenance,
            } => self.commit_rename_party(id, name, provenance),
            Op::SetPartyKind {
                id,
                kind,
                provenance,
            } => self.commit_set_party_kind(id, kind, provenance),
            Op::RemoveParty { id } => self.commit_remove_party(id),
            Op::AddElement {
                id,
                kind,
                name,
                provenance,
            } => Ok(self.commit_add_element(id, kind, name, provenance)),
            Op::RenameElement {
                id,
                name,
                provenance,
            } => self.commit_rename_element(id, name, provenance),
            Op::RemoveElement { id } => self.commit_remove_element(id),
            Op::SetElementAttribute {
                id,
                key,
                value,
                provenance,
            } => self.commit_set_element_attribute(id, key, value, provenance),
            Op::LinkElement {
                id,
                target,
                provenance,
            } => self.commit_link_element(id, target, provenance),
            Op::UnlinkElement {
                id,
                target,
                provenance,
            } => self.commit_unlink_element(id, target, provenance),
            Op::SetThreatGoal {
                threat,
                goal,
                provenance,
            } => self.commit_set_threat_goal(threat, goal, provenance),
            Op::SetThreatClaim {
                threat,
                claim,
                provenance,
            } => self.commit_set_threat_claim(threat, claim, provenance),
            Op::SetThreatScaling {
                threat,
                scales,
                reason,
                provenance,
            } => self.commit_set_threat_scaling(threat, scales, reason, provenance),
            Op::SetImpactRationale {
                damage,
                text,
                provenance,
            } => {
                let idx = self.damage_index(damage)?;
                let d = self
                    .damages
                    .get_mut(idx)
                    .ok_or(ModelError::UnknownDamageScenario(damage))?;
                Ok(Op::SetImpactRationale {
                    damage,
                    text: std::mem::replace(&mut d.impact_rationale, text),
                    provenance: std::mem::replace(&mut d.provenance, provenance),
                })
            }
            Op::SetFeasibilityRationale {
                threat,
                text,
                provenance,
            } => {
                let idx = self.threat_index(threat)?;
                let t = self
                    .threats
                    .get_mut(idx)
                    .ok_or(ModelError::UnknownThreatScenario(threat))?;
                Ok(Op::SetFeasibilityRationale {
                    threat,
                    text: std::mem::replace(&mut t.feasibility_rationale, text),
                    provenance: std::mem::replace(&mut t.provenance, provenance),
                })
            }
            Op::SetDamageLinkRationale {
                threat,
                damage,
                text,
                provenance,
            } => {
                let idx = self.threat_index(threat)?;
                let t = self
                    .threats
                    .get(idx)
                    .ok_or(ModelError::UnknownThreatScenario(threat))?;
                let pos = self
                    .find_link(t, damage)
                    .map_err(|_| ModelError::DamageNotLinked { threat, damage })?;
                let dmg = t.damages[pos];
                let key = self.damage_key(dmg);
                let overlay = t
                    .damage_rationales
                    .binary_search_by_key(&key, |(d, _)| self.damage_key(*d));
                let t = self
                    .threats
                    .get_mut(idx)
                    .ok_or(ModelError::UnknownThreatScenario(threat))?;
                let previous = match overlay {
                    Ok(i) if text.is_empty() => t.damage_rationales.remove(i).1,
                    Ok(i) => std::mem::replace(&mut t.damage_rationales[i].1, text),
                    Err(i) => {
                        if !text.is_empty() {
                            t.damage_rationales.insert(i, (dmg, text));
                        }
                        Arc::from("")
                    }
                };
                let provenance = std::mem::replace(&mut t.provenance, provenance);
                Ok(Op::SetDamageLinkRationale {
                    threat,
                    damage,
                    text: previous,
                    provenance,
                })
            }
            Op::AddComponent {
                id,
                name,
                parent,
                provenance,
            } => self.commit_add_component(id, name, parent, provenance),
            Op::RenameComponent {
                id,
                name,
                provenance,
            } => self.commit_rename_component(id, name, provenance),
            Op::SetComponentParent {
                id,
                parent,
                provenance,
            } => self.commit_set_component_parent(id, parent, provenance),
            Op::SetComponentSystem {
                id,
                system,
                provenance,
            } => self.commit_set_component_system(id, system, provenance),
            Op::RemoveComponent { id } => self.commit_remove_component(id),
            Op::AddInterface {
                id,
                name,
                component,
                kind,
                provenance,
            } => self.commit_add_interface(id, name, component, kind, provenance),
            Op::RenameInterface {
                id,
                name,
                provenance,
            } => self.commit_rename_interface(id, name, provenance),
            Op::SetInterfaceKind {
                id,
                kind,
                provenance,
            } => self.commit_set_interface_kind(id, kind, provenance),
            Op::RemoveInterface { id } => self.commit_remove_interface(id),
            Op::AddDataFlow {
                id,
                name,
                from,
                to,
                interface,
                provenance,
            } => self.commit_add_data_flow(id, name, from, to, interface, provenance),
            Op::RenameDataFlow {
                id,
                name,
                provenance,
            } => self.commit_rename_data_flow(id, name, provenance),
            Op::SetDataFlowInterface {
                id,
                interface,
                provenance,
            } => self.commit_set_data_flow_interface(id, interface, provenance),
            Op::RemoveDataFlow { id } => self.commit_remove_data_flow(id),
            Op::AddTrustBoundary {
                id,
                name,
                components,
                provenance,
            } => self.commit_add_trust_boundary(id, name, components, provenance),
            Op::RenameTrustBoundary {
                id,
                name,
                provenance,
            } => self.commit_rename_trust_boundary(id, name, provenance),
            Op::AddBoundaryComponent {
                boundary,
                component,
                provenance,
            } => self.commit_boundary_component(boundary, component, provenance, true),
            Op::RemoveBoundaryComponent {
                boundary,
                component,
                provenance,
            } => self.commit_boundary_component(boundary, component, provenance, false),
            Op::RemoveTrustBoundary { id } => self.commit_remove_trust_boundary(id),
            Op::SetAssetComponent {
                asset,
                component,
                provenance,
            } => self.commit_set_asset_component(asset, component, provenance),
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

    fn goal_mut(&mut self, id: GoalId) -> Result<&mut CybersecurityGoal, ModelError> {
        let idx = self.goal_index(id)?;
        self.goals
            .get_mut(idx)
            .ok_or(ModelError::UnknownCybersecurityGoal(id))
    }

    fn claim_mut(&mut self, id: ClaimId) -> Result<&mut CybersecurityClaim, ModelError> {
        let idx = self.claim_index(id)?;
        self.claims
            .get_mut(idx)
            .ok_or(ModelError::UnknownCybersecurityClaim(id))
    }

    fn party_mut(&mut self, id: PartyId) -> Result<&mut Party, ModelError> {
        let idx = self.party_index(id)?;
        self.parties
            .get_mut(idx)
            .ok_or(ModelError::UnknownParty(id))
    }

    fn component_mut(&mut self, id: ComponentId) -> Result<&mut Component, ModelError> {
        let idx = self.component_index(id)?;
        self.components
            .get_mut(idx)
            .ok_or(ModelError::UnknownComponent(id))
    }

    fn interface_mut(&mut self, id: InterfaceId) -> Result<&mut Interface, ModelError> {
        let idx = self.interface_index(id)?;
        self.interfaces
            .get_mut(idx)
            .ok_or(ModelError::UnknownInterface(id))
    }

    fn data_flow_mut(&mut self, id: DataFlowId) -> Result<&mut DataFlow, ModelError> {
        let idx = self.data_flow_index(id)?;
        self.data_flows
            .get_mut(idx)
            .ok_or(ModelError::UnknownDataFlow(id))
    }

    fn trust_boundary_mut(
        &mut self,
        id: TrustBoundaryId,
    ) -> Result<&mut TrustBoundary, ModelError> {
        let idx = self.trust_boundary_index(id)?;
        self.trust_boundaries
            .get_mut(idx)
            .ok_or(ModelError::UnknownTrustBoundary(id))
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

    fn goal_index(&self, id: GoalId) -> Result<GoalIdx, ModelError> {
        self.goals
            .idx(&id)
            .ok_or(ModelError::UnknownCybersecurityGoal(id))
    }

    fn claim_index(&self, id: ClaimId) -> Result<ClaimIdx, ModelError> {
        self.claims
            .idx(&id)
            .ok_or(ModelError::UnknownCybersecurityClaim(id))
    }

    fn party_index(&self, id: PartyId) -> Result<PartyIdx, ModelError> {
        self.parties.idx(&id).ok_or(ModelError::UnknownParty(id))
    }

    fn component_index(&self, id: ComponentId) -> Result<ComponentIdx, ModelError> {
        self.components
            .idx(&id)
            .ok_or(ModelError::UnknownComponent(id))
    }

    fn interface_index(&self, id: InterfaceId) -> Result<InterfaceIdx, ModelError> {
        self.interfaces
            .idx(&id)
            .ok_or(ModelError::UnknownInterface(id))
    }

    fn data_flow_index(&self, id: DataFlowId) -> Result<DataFlowIdx, ModelError> {
        self.data_flows
            .idx(&id)
            .ok_or(ModelError::UnknownDataFlow(id))
    }

    fn trust_boundary_index(&self, id: TrustBoundaryId) -> Result<TrustBoundaryIdx, ModelError> {
        self.trust_boundaries
            .idx(&id)
            .ok_or(ModelError::UnknownTrustBoundary(id))
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

    fn live_goal(&self, id: GoalId) -> Result<(GoalIdx, &CybersecurityGoal), ModelError> {
        let idx = self.goal_index(id)?;
        let g = self
            .goals
            .get(idx)
            .ok_or(ModelError::UnknownCybersecurityGoal(id))?;
        Ok((idx, g))
    }

    fn live_claim_ref(&self, id: ClaimId) -> Result<&CybersecurityClaim, ModelError> {
        self.claims
            .get(self.live_claim(id)?)
            .ok_or(ModelError::UnknownCybersecurityClaim(id))
    }

    fn live_claim(&self, id: ClaimId) -> Result<ClaimIdx, ModelError> {
        let idx = self.claim_index(id)?;
        self.claims
            .get(idx)
            .map(|_| idx)
            .ok_or(ModelError::UnknownCybersecurityClaim(id))
    }

    fn live_component(&self, id: ComponentId) -> Result<(ComponentIdx, &Component), ModelError> {
        let idx = self.component_index(id)?;
        let c = self
            .components
            .get(idx)
            .ok_or(ModelError::UnknownComponent(id))?;
        Ok((idx, c))
    }

    fn live_interface(&self, id: InterfaceId) -> Result<(InterfaceIdx, &Interface), ModelError> {
        let idx = self.interface_index(id)?;
        let i = self
            .interfaces
            .get(idx)
            .ok_or(ModelError::UnknownInterface(id))?;
        Ok((idx, i))
    }

    fn live_data_flow(&self, id: DataFlowId) -> Result<(DataFlowIdx, &DataFlow), ModelError> {
        let idx = self.data_flow_index(id)?;
        let df = self
            .data_flows
            .get(idx)
            .ok_or(ModelError::UnknownDataFlow(id))?;
        Ok((idx, df))
    }

    fn live_trust_boundary(
        &self,
        id: TrustBoundaryId,
    ) -> Result<(TrustBoundaryIdx, &TrustBoundary), ModelError> {
        let idx = self.trust_boundary_index(id)?;
        let tb = self
            .trust_boundaries
            .get(idx)
            .ok_or(ModelError::UnknownTrustBoundary(id))?;
        Ok((idx, tb))
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

    fn party_key(&self, idx: PartyIdx) -> Option<PartyId> {
        self.parties.id(idx)
    }

    /// The public form of a stored "through" item, if it is live.
    pub(crate) fn item_ref(&self, idx: Option<ItemIdx>) -> Option<ItemRef> {
        match idx? {
            ItemIdx::Component(c) => self.components.id(c).map(ItemRef::Component),
            ItemIdx::Interface(i) => self.interfaces.id(i).map(ItemRef::Interface),
        }
    }

    /// The index form of a public "through" item, if it is live.
    fn item_idx(&self, item: ItemRef) -> Result<ItemIdx, ModelError> {
        Ok(match item {
            ItemRef::Component(id) => ItemIdx::Component(self.live_component(id)?.0),
            ItemRef::Interface(id) => ItemIdx::Interface(self.live_interface(id)?.0),
        })
    }

    /// How many attack steps go through `item` itself, not through its
    /// interfaces. A scan over all step slots, O(total nodes).
    fn count_steps_via(&self, item: ItemIdx) -> u32 {
        let n = self
            .trees
            .iter()
            .flat_map(|(_, _, e)| e.vias())
            .filter(|(_, v)| *v == item)
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// The public ID of a linked cybersecurity goal, if any.
    fn goal_key(&self, idx: Option<GoalIdx>) -> Option<GoalId> {
        idx.and_then(|i| self.goals.id(i))
    }

    /// The public ID of a linked cybersecurity claim, if any.
    fn claim_key(&self, idx: Option<ClaimIdx>) -> Option<ClaimId> {
        idx.and_then(|i| self.claims.id(i))
    }

    /// The public ID of a referenced component.
    fn component_key(&self, idx: ComponentIdx) -> Option<ComponentId> {
        self.components.id(idx)
    }

    /// The public ID of an optional referenced component.
    fn component_opt_key(&self, idx: Option<ComponentIdx>) -> Option<ComponentId> {
        idx.and_then(|i| self.components.id(i))
    }

    /// The public ID of an optional referenced interface.
    fn interface_opt_key(&self, idx: Option<InterfaceIdx>) -> Option<InterfaceId> {
        idx.and_then(|i| self.interfaces.id(i))
    }

    /// Finds `component` in a trust boundary's sorted components: `Ok(pos)` if
    /// present, `Err(pos)` for the insertion point. O(log m).
    fn find_boundary_component(
        &self,
        tb: &TrustBoundary,
        component: ComponentId,
    ) -> Result<usize, usize> {
        tb.components
            .binary_search_by(|&c| self.component_key(c).cmp(&Some(component)))
    }

    /// Finds `control` in a step's sorted mitigations: `Ok(pos)` if attached,
    /// `Err(pos)` for the insertion point. O(log m).
    fn find_mitigation(&self, row: &[StepMitigation], control: ControlId) -> Result<usize, usize> {
        row.binary_search_by(|m| self.control_key(m.control).cmp(&Some(control)))
    }

    /// Finds `control` in a goal's sorted controls: `Ok(pos)` if linked,
    /// `Err(pos)` for the insertion point. O(log c).
    fn find_goal_control(&self, g: &CybersecurityGoal, control: ControlId) -> Result<usize, usize> {
        g.controls
            .binary_search_by(|&c| self.control_key(c).cmp(&Some(control)))
    }

    fn find_goal_party(&self, g: &CybersecurityGoal, party: PartyId) -> Result<usize, usize> {
        g.responsible
            .binary_search_by(|&p| self.party_key(p).cmp(&Some(party)))
    }

    fn find_claim_party(&self, c: &CybersecurityClaim, party: PartyId) -> Result<usize, usize> {
        c.responsible
            .binary_search_by(|&p| self.party_key(p).cmp(&Some(party)))
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

    /// A live tree and a node in it.
    fn tree_node(&self, tree: TreeId, node: NodeId) -> Result<(&TreeEntry, NodeIdx), ModelError> {
        let (_, entry) = self.live_tree(tree)?;
        let idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        Ok((entry, idx))
    }

    #[allow(clippy::too_many_arguments)]
    fn check_add_tree_node(
        &self,
        tree: TreeId,
        parent: NodeId,
        id: NodeId,
        node: &Node,
        index: u32,
        position: u32,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        let (entry, parent_idx) = self.tree_node(tree, parent)?;
        if entry.tree.len() >= MAX_ATTACK_TREE_NODES {
            return Err(ModelError::TooManyAttackTreeNodes {
                tree,
                max: MAX_ATTACK_TREE_NODES,
            });
        }
        check_name(EntityKind::TreeNode, node.label())?;
        check_provenance(provenance)?;
        let at = NodeIdx::try_from_usize(index as usize).ok_or(ModelError::BadNodePlacement)?;
        entry
            .tree
            .check_insert(parent_idx, at, position as usize, id, node)
    }

    fn check_remove_tree_node(
        &self,
        tree: TreeId,
        id: NodeId,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        let (entry, idx) = self.tree_node(tree, id)?;
        entry.tree.check_remove(idx)?;
        let rated = self.review_marks.contains(&(tree, id))
            || entry
                .step_feasibility
                .get(idx.index())
                .is_some_and(Option::is_some)
            || entry
                .step_factors
                .get(idx.index())
                .is_some_and(|row| row.iter().any(Option::is_some))
            || entry
                .step_mitigations
                .get(idx.index())
                .is_some_and(|m| !m.is_empty())
            || entry.via_at(idx.index()).is_some()
            || entry.attack_type_at(idx.index()).is_some();
        if rated {
            return Err(ModelError::NodeIsRated(id));
        }
        check_provenance(provenance)
    }

    fn check_link_tree_node(
        &self,
        tree: TreeId,
        parent: NodeId,
        child: NodeId,
        position: u32,
        slot: u32,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        let (entry, parent_idx) = self.tree_node(tree, parent)?;
        let child_idx = entry
            .tree
            .idx_of(&child)
            .ok_or(ModelError::UnknownTreeNode { tree, node: child })?;
        entry
            .tree
            .check_link(parent_idx, child_idx, position as usize, slot as usize)?;
        check_provenance(provenance)
    }

    fn check_unlink_tree_node(
        &self,
        tree: TreeId,
        parent: NodeId,
        child: NodeId,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        let (entry, parent_idx) = self.tree_node(tree, parent)?;
        let child_idx = entry
            .tree
            .idx_of(&child)
            .ok_or(ModelError::UnknownTreeNode { tree, node: child })?;
        entry.tree.check_unlink(parent_idx, child_idx)?;
        check_provenance(provenance)
    }

    #[allow(clippy::too_many_arguments)]
    fn commit_add_tree_node(
        &mut self,
        tree: TreeId,
        parent: NodeId,
        id: NodeId,
        node: Node,
        index: u32,
        position: u32,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let entry = self.tree_mut(tree)?;
        let parent_idx = entry
            .tree
            .idx_of(&parent)
            .ok_or(ModelError::UnknownTreeNode { tree, node: parent })?;
        let at = NodeIdx::try_from_usize(index as usize).ok_or(ModelError::BadNodePlacement)?;
        entry
            .tree
            .insert_child(parent_idx, at, position as usize, id, node)?;
        entry.step_feasibility.insert(at.index(), None);
        entry.step_factors.insert(at.index(), Box::default());
        entry.step_mitigations.insert(at.index(), Box::default());
        entry.insert_links(at.index());
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::RemoveTreeNode {
            tree,
            id,
            provenance,
        })
    }

    fn commit_remove_tree_node(
        &mut self,
        tree: TreeId,
        id: NodeId,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let entry = self.tree_mut(tree)?;
        let idx = entry
            .tree
            .idx_of(&id)
            .ok_or(ModelError::UnknownTreeNode { tree, node: id })?;
        let removed = entry.tree.remove_leaf(idx)?;
        entry.step_feasibility.remove(idx.index());
        entry.step_factors.remove(idx.index());
        entry.step_mitigations.remove(idx.index());
        entry.remove_links(idx.index());
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        let parent = entry
            .tree
            .id_of(removed.owner)
            .ok_or(ModelError::UnknownNode(removed.owner))?;
        Ok(Op::AddTreeNode {
            tree,
            parent,
            id,
            node: removed.node,
            index: u32::try_from(removed.index.index()).unwrap_or(u32::MAX),
            position: u32::try_from(removed.position).unwrap_or(u32::MAX),
            provenance,
        })
    }

    fn commit_link_tree_node(
        &mut self,
        tree: TreeId,
        parent: NodeId,
        child: NodeId,
        position: u32,
        slot: u32,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let entry = self.tree_mut(tree)?;
        let (Some(p), Some(c)) = (entry.tree.idx_of(&parent), entry.tree.idx_of(&child)) else {
            return Err(ModelError::UnknownTreeNode { tree, node: parent });
        };
        entry.tree.link_at(p, c, position as usize, slot as usize)?;
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::UnlinkTreeNode {
            tree,
            parent,
            child,
            provenance,
        })
    }

    fn commit_unlink_tree_node(
        &mut self,
        tree: TreeId,
        parent: NodeId,
        child: NodeId,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let entry = self.tree_mut(tree)?;
        let (Some(p), Some(c)) = (entry.tree.idx_of(&parent), entry.tree.idx_of(&child)) else {
            return Err(ModelError::UnknownTreeNode { tree, node: parent });
        };
        let (position, slot) = entry.tree.unlink(p, c)?;
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::LinkTreeNode {
            tree,
            parent,
            child,
            position: u32::try_from(position).unwrap_or(u32::MAX),
            slot: u32::try_from(slot).unwrap_or(u32::MAX),
            provenance,
        })
    }

    /// Checks that the element a description names exists (or that it is
    /// the project itself).
    fn live_target(&self, target: ElementRef) -> Result<(), ModelError> {
        match target {
            ElementRef::Project | ElementRef::ItemDescription | ElementRef::ItemScope => Ok(()),
            ElementRef::Asset(id) => self.live_asset(id).map(|_| ()),
            ElementRef::Damage(id) => self.live_damage(id).map(|_| ()),
            ElementRef::Threat(id) => self.live_threat(id).map(|_| ()),
            ElementRef::Tree(id) => self.live_tree(id).map(|_| ()),
            ElementRef::Control(id) => self.live_control(id).map(|_| ()),
            ElementRef::Goal(id) => self.live_goal(id).map(|_| ()),
            ElementRef::Claim(id) => self.live_claim(id).map(|_| ()),
            ElementRef::Component(id) => self.live_component(id).map(|_| ()),
            ElementRef::Interface(id) => self.live_interface(id).map(|_| ()),
            ElementRef::DataFlow(id) => self.live_data_flow(id).map(|_| ()),
            ElementRef::TrustBoundary(id) => self.live_trust_boundary(id).map(|_| ()),
        }
    }

    /// Checks a description change. Time O(text length).
    fn check_set_description(
        &self,
        target: ElementRef,
        text: &str,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        self.live_target(target)?;
        check_text(text, target.max_chars())?;
        check_provenance(provenance)?;
        let old = self.descriptions.get(&target).map_or(0, |d| d.text.len());
        if self.description_bytes - old + text.len() > MAX_DESCRIPTION_TOTAL_BYTES {
            return Err(ModelError::DescriptionTotalTooLarge {
                max: MAX_DESCRIPTION_TOTAL_BYTES,
            });
        }
        Ok(())
    }

    /// An element with a description must have it cleared first.
    fn require_no_description(
        &self,
        target: ElementRef,
        kind: EntityKind,
    ) -> Result<(), ModelError> {
        if self.descriptions.contains_key(&target) {
            return Err(ModelError::DescriptionNotCleared { kind });
        }
        Ok(())
    }

    /// Stores or clears a description and returns the inverse. Time
    /// O(log n + text length).
    fn commit_set_description(
        &mut self,
        target: ElementRef,
        text: Arc<str>,
        provenance: Provenance,
    ) -> Op {
        let old = self.descriptions.remove(&target);
        let (old_text, old_provenance, translations) = old.map_or_else(
            || (Arc::<str>::from(""), Provenance::default(), Vec::new()),
            |d| (d.text, d.provenance, d.translations),
        );
        self.description_bytes -= old_text.len();
        if text.is_empty() {
            for t in &translations {
                self.description_bytes -= t.text.len();
            }
        } else {
            self.description_bytes += text.len();
            self.descriptions.insert(
                target,
                Described {
                    text,
                    translations,
                    provenance,
                },
            );
        }
        Op::SetDescription {
            target,
            text: old_text,
            provenance: old_provenance,
        }
    }

    /// The description of an element, or the notes of the project
    /// ([`ElementRef::Project`]). `None` when there is none. Time O(log n).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ElementRef, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// assert_eq!(project.description(ElementRef::Project), None);
    /// project.apply(Op::SetDescription {
    ///     target: ElementRef::Project,
    ///     text: "Scope: the brake ECU.".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// assert_eq!(project.description(ElementRef::Project), Some("Scope: the brake ECU."));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn description(&self, target: ElementRef) -> Option<&str> {
        self.descriptions.get(&target).map(|d| &*d.text)
    }

    /// Who wrote the description and when. Time O(log n).
    #[must_use]
    pub fn description_provenance(&self, target: ElementRef) -> Option<&Provenance> {
        self.descriptions.get(&target).map(|d| &d.provenance)
    }

    /// Checks a description translation change. The description must exist,
    /// the tag must be a short BCP 47 tag, and the text must fit (FND-04).
    fn check_set_description_translation(
        &self,
        target: ElementRef,
        lang: &str,
        text: &str,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        self.live_target(target)?;
        check_lang(lang)?;
        check_text(text, target.max_chars())?;
        check_provenance(provenance)?;
        let described = self
            .descriptions
            .get(&target)
            .ok_or(ModelError::NoDescription)?;
        let old = described
            .translations
            .iter()
            .find(|t| &*t.lang == lang)
            .map_or(0, |t| t.text.len());
        if self.description_bytes - old + text.len() > MAX_DESCRIPTION_TOTAL_BYTES {
            return Err(ModelError::DescriptionTotalTooLarge {
                max: MAX_DESCRIPTION_TOTAL_BYTES,
            });
        }
        if !text.is_empty()
            && !described.translations.iter().any(|t| &*t.lang == lang)
            && described.translations.len() >= MAX_DESCRIPTION_TRANSLATIONS
        {
            return Err(ModelError::TooManyTranslations {
                max: MAX_DESCRIPTION_TRANSLATIONS,
            });
        }
        Ok(())
    }

    /// Stores or clears one translation and returns the inverse (FND-04).
    fn commit_set_description_translation(
        &mut self,
        target: ElementRef,
        lang: Arc<str>,
        text: Arc<str>,
        provenance: Provenance,
    ) -> Op {
        let described = self
            .descriptions
            .get_mut(&target)
            .expect("the description exists: validate checked it");
        let old = if text.is_empty() {
            if let Some(i) = described.translations.iter().position(|t| t.lang == lang) {
                let removed = described.translations.remove(i);
                self.description_bytes -= removed.text.len();
                Some(removed.text)
            } else {
                None
            }
        } else {
            self.description_bytes += text.len();
            if let Some(t) = described.translations.iter_mut().find(|t| t.lang == lang) {
                self.description_bytes -= t.text.len();
                Some(std::mem::replace(&mut t.text, text))
            } else {
                described.translations.push(Translation {
                    lang: lang.clone(),
                    text,
                });
                None
            }
        };
        Op::SetDescriptionTranslation {
            target,
            lang,
            text: old.unwrap_or_else(|| Arc::from("")),
            provenance,
        }
    }

    /// One translation of a description, if set (FND-04). Time O(log n).
    #[must_use]
    pub fn description_translation(&self, target: ElementRef, lang: &str) -> Option<&str> {
        self.descriptions
            .get(&target)?
            .translations
            .iter()
            .find(|t| &*t.lang == lang)
            .map(|t| &*t.text)
    }

    /// Every translation of a description, in insertion order, as `(lang,
    /// text)` (FND-04). Time O(log n) to start.
    pub fn description_translations(
        &self,
        target: ElementRef,
    ) -> impl Iterator<Item = (&str, &str)> {
        self.descriptions
            .get(&target)
            .into_iter()
            .flat_map(|d| d.translations.iter().map(|t| (&*t.lang, &*t.text)))
    }

    /// Whether the rating of a step is marked as needing review. Time
    /// O(log n).
    #[must_use]
    pub fn review_marked(&self, tree: TreeId, node: NodeId) -> bool {
        self.review_marks.contains(&(tree, node))
    }

    /// The languages the project keeps description translations in, in
    /// display order; empty for a single-language project. Time O(1).
    #[must_use]
    pub fn languages(&self) -> &[Arc<str>] {
        &self.languages.0
    }

    /// Every step marked as needing review, in ID order. Time O(1) to start.
    pub fn review_marks(&self) -> impl Iterator<Item = (TreeId, NodeId)> + '_ {
        self.review_marks.iter().copied()
    }

    /// How many descriptions the project has, notes included. Time O(1).
    #[must_use]
    pub fn description_count(&self) -> usize {
        self.descriptions.len()
    }

    /// Every description in ID order: the target, the text and the
    /// provenance. Time O(1) to start, O(1) per item.
    pub fn descriptions(&self) -> impl Iterator<Item = (ElementRef, &str, &Provenance)> {
        self.descriptions
            .iter()
            .map(|(target, d)| (*target, &*d.text, &d.provenance))
    }

    fn check_remove_asset(&self, id: AssetId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::Asset(id), EntityKind::Asset)?;
        let (idx, asset) = self.live_asset(id)?;
        if let Some(comp_idx) = asset.component {
            let component = self
                .components
                .id(comp_idx)
                .ok_or(ModelError::UnknownAsset(id))?;
            return Err(ModelError::AssetHasComponent {
                asset: id,
                component,
            });
        }
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
        self.require_no_description(ElementRef::Damage(id), EntityKind::DamageScenario)?;
        let idx = self.live_damage(id)?;
        if self
            .damages
            .get(idx)
            .is_some_and(|d| !d.impact_rationale.is_empty())
        {
            return Err(ModelError::DamageHasRationale { damage: id });
        }
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
        if let Some(w) = refs.windows(2).find(|w| w.first() == w.get(1))
            && let Some(&catalog_ref) = w.first()
        {
            return Err(ModelError::DuplicateCatalogRef {
                threat: id,
                catalog_ref,
            });
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
        if let Some(w) = ids.windows(2).find(|w| w.first() == w.get(1))
            && let Some(&damage) = w.first()
        {
            return Err(ModelError::DuplicateDamageLink { threat: id, damage });
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
        self.require_no_description(ElementRef::Threat(id), EntityKind::ThreatScenario)?;
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
        if let Some(goal_idx) = t.goal {
            let goal = self
                .goals
                .id(goal_idx)
                .ok_or(ModelError::UnknownThreatScenario(id))?;
            return Err(ModelError::ThreatHasGoal { threat: id, goal });
        }
        if let Some(claim_idx) = t.claim {
            let claim = self
                .claims
                .id(claim_idx)
                .ok_or(ModelError::UnknownThreatScenario(id))?;
            return Err(ModelError::ThreatHasClaim { threat: id, claim });
        }
        if t.scales.is_some() || !t.scaling_reason.is_empty() {
            return Err(ModelError::ThreatHasScaling { threat: id });
        }
        if !t.feasibility_rationale.is_empty() {
            return Err(ModelError::ThreatHasRationale { threat: id });
        }
        let elements = self
            .elements
            .inbound(crate::element::LinkTarget::Threat(id));
        if elements > 0 {
            return Err(ModelError::ThreatLinkedFromElements {
                threat: id,
                elements,
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
        self.check_add_tree_max(id, tree, step_feasibility, provenance, MAX_TREE_OP_NODES)
    }

    /// Loads a whole tree from a snapshot. An op carries at most
    /// [`MAX_TREE_OP_NODES`] nodes, a snapshot up to [`MAX_ATTACK_TREE_NODES`];
    /// every other check is the same as for `AddAttackTree`.
    pub(crate) fn load_attack_tree(
        &mut self,
        id: TreeId,
        tree: AttackTree,
        step_feasibility: Vec<(NodeId, FeasibilityLevel)>,
        provenance: Provenance,
    ) -> Result<(), ModelError> {
        self.check_add_tree_max(
            id,
            &tree,
            &step_feasibility,
            &provenance,
            MAX_ATTACK_TREE_NODES,
        )?;
        self.commit_add_tree(id, tree, step_feasibility, provenance)
            .map(|_| ())
    }

    fn check_add_tree_max(
        &self,
        id: TreeId,
        tree: &AttackTree,
        step_feasibility: &[(NodeId, FeasibilityLevel)],
        provenance: &Provenance,
        max_nodes: usize,
    ) -> Result<(), ModelError> {
        if self.trees.idx(&id).is_some() {
            return Err(ModelError::DuplicateAttackTree(id));
        }
        if tree.len() > max_nodes {
            return Err(ModelError::TooManyAttackTreeNodes {
                tree: id,
                max: max_nodes,
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

    /// The tree and the step slot of `node`, or why it is not a step.
    fn step_slot(&self, tree: TreeId, node: NodeId) -> Result<NodeIdx, ModelError> {
        let (_, entry) = self.live_tree(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        if !matches!(entry.tree.get(node_idx), Some(Node::Step { .. })) {
            return Err(ModelError::NotAStep { tree, node });
        }
        Ok(node_idx)
    }

    fn check_set_step_via(
        &self,
        tree: TreeId,
        node: NodeId,
        via: Option<ItemRef>,
    ) -> Result<(), ModelError> {
        self.step_slot(tree, node)?;
        if let Some(item) = via {
            self.item_idx(item)?;
        }
        Ok(())
    }

    fn check_set_step_attack_type(
        &self,
        tree: TreeId,
        node: NodeId,
        attack_type: Option<AttackType>,
    ) -> Result<(), ModelError> {
        self.step_slot(tree, node)?;
        if let Some(t) = attack_type {
            self.methodology.check_attack_type(t)?;
        }
        Ok(())
    }

    fn check_set_step_factor(
        &self,
        tree: TreeId,
        node: NodeId,
        factor: FeasibilityFactor,
        level: Option<FactorLevel>,
    ) -> Result<(), ModelError> {
        let (_, entry) = self.live_tree(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        if !matches!(entry.tree.get(node_idx), Some(Node::Step { .. })) {
            return Err(ModelError::NotAStep { tree, node });
        }
        match level {
            Some(level) => self.methodology.check_factor_level(factor, level)?,
            None => self.methodology.check_feasibility_factor(factor)?,
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
        if let Some(&(_, node)) = self.review_marks.iter().find(|(t, _)| *t == id) {
            return Err(ModelError::NodeIsRated(node));
        }
        if let Some((_, entry)) = self
            .trees
            .idx(&id)
            .and_then(|i| self.trees.get(i).map(|e| (i, e)))
            && entry.tree.len() > MAX_TREE_OP_NODES
        {
            return Err(ModelError::TreeTooLargeForOneOp {
                tree: id,
                max: MAX_TREE_OP_NODES,
            });
        }
        self.require_no_description(ElementRef::Tree(id), EntityKind::AttackTree)?;
        let (idx, entry) = self.live_tree(id)?;
        let threats = self.refs.trees.get(idx.index()).copied().unwrap_or(0);
        if threats > 0 {
            return Err(ModelError::AttackTreeInUse { tree: id, threats });
        }
        let factors: usize = entry
            .step_factors
            .iter()
            .flat_map(|r| r.iter())
            .filter(|l| l.is_some())
            .count();
        if factors > 0 {
            return Err(ModelError::AttackTreeHasFactors { tree: id, factors });
        }
        let links = entry.linked_steps();
        if links > 0 {
            return Err(ModelError::AttackTreeHasStepLinks {
                tree: id,
                steps: links,
            });
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

    fn find_control_target(&self, c: &SecurityControl, target: ItemRef) -> Result<usize, usize> {
        c.targets
            .binary_search_by(|&t| self.item_ref(Some(t)).cmp(&Some(target)))
    }

    fn check_control_target(
        &self,
        control: ControlId,
        target: ItemRef,
        apply: bool,
    ) -> Result<(), ModelError> {
        let idx = self.live_control(control)?;
        let c = self
            .controls
            .get(idx)
            .ok_or(ModelError::UnknownSecurityControl(control))?;
        self.item_idx(target)?;
        match (apply, self.find_control_target(c, target).is_ok()) {
            (true, true) => Err(ModelError::DuplicateControlTarget { control, target }),
            (true, false) if c.targets.len() >= MAX_CONTROL_TARGETS => {
                Err(ModelError::TooManyControlTargets {
                    control,
                    max: MAX_CONTROL_TARGETS,
                })
            }
            (false, false) => Err(ModelError::ControlTargetNotLinked { control, target }),
            _ => Ok(()),
        }
    }

    fn check_set_control_rule(
        &self,
        control: ControlId,
        attack_type: AttackType,
        ceiling: Option<FeasibilityLevel>,
    ) -> Result<(), ModelError> {
        let idx = self.live_control(control)?;
        let c = self
            .controls
            .get(idx)
            .ok_or(ModelError::UnknownSecurityControl(control))?;
        self.methodology.check_attack_type(attack_type)?;
        if let Some(level) = ceiling {
            self.methodology.check_feasibility_level(level)?;
            if c.rules
                .binary_search_by_key(&attack_type, |r| r.attack_type)
                .is_err()
                && c.rules.len() >= MAX_CONTROL_RULES
            {
                return Err(ModelError::TooManyControlRules {
                    control,
                    max: MAX_CONTROL_RULES,
                });
            }
        }
        Ok(())
    }

    fn check_set_control_kind(
        &self,
        control: ControlId,
        kind: Option<ControlKind>,
    ) -> Result<(), ModelError> {
        self.live_control(control)?;
        if let Some(kind) = kind {
            self.methodology.check_control_kind(kind)?;
        }
        Ok(())
    }

    fn check_remove_control(&self, id: ControlId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::Control(id), EntityKind::SecurityControl)?;
        let idx = self.live_control(id)?;
        let steps = self.refs.controls.get(idx.index()).copied().unwrap_or(0);
        if steps > 0 {
            return Err(ModelError::SecurityControlInUse { control: id, steps });
        }
        let goals = self
            .refs
            .control_goals
            .get(idx.index())
            .copied()
            .unwrap_or(0);
        if goals > 0 {
            return Err(ModelError::SecurityControlInGoal { control: id, goals });
        }
        let c = self
            .controls
            .get(idx)
            .ok_or(ModelError::UnknownSecurityControl(id))?;
        if !c.targets.is_empty() {
            return Err(ModelError::SecurityControlHasTargets {
                control: id,
                targets: c.targets.len(),
            });
        }
        if !c.rules.is_empty() {
            return Err(ModelError::SecurityControlHasRules {
                control: id,
                rules: c.rules.len(),
            });
        }
        if c.kind.is_some() {
            return Err(ModelError::SecurityControlHasKind { control: id });
        }
        Ok(())
    }

    fn check_add_goal(
        &self,
        id: GoalId,
        name: &str,
        controls: &[ControlId],
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.goals.idx(&id).is_some() {
            return Err(ModelError::DuplicateCybersecurityGoal(id));
        }
        check_name(EntityKind::CybersecurityGoal, name)?;
        if controls.len() > MAX_GOAL_CONTROLS {
            return Err(ModelError::TooManyGoalControls {
                goal: id,
                max: MAX_GOAL_CONTROLS,
            });
        }
        for &c in controls {
            self.live_control(c)?;
        }
        let mut ids = controls.to_vec();
        ids.sort_unstable();
        if let Some(&control) = ids
            .windows(2)
            .find(|w| w.first() == w.get(1))
            .and_then(|w| w.first())
        {
            return Err(ModelError::DuplicateGoalControl { goal: id, control });
        }
        check_provenance(provenance)?;
        if self.goals.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::CybersecurityGoal,
            });
        }
        Ok(())
    }

    fn check_goal_control(
        &self,
        goal: GoalId,
        control: ControlId,
        link: bool,
    ) -> Result<(), ModelError> {
        let (_, g) = self.live_goal(goal)?;
        self.live_control(control)?;
        match (link, self.find_goal_control(g, control).is_ok()) {
            (true, true) => Err(ModelError::DuplicateGoalControl { goal, control }),
            (true, false) if g.controls.len() >= MAX_GOAL_CONTROLS => {
                Err(ModelError::TooManyGoalControls {
                    goal,
                    max: MAX_GOAL_CONTROLS,
                })
            }
            (false, false) => Err(ModelError::GoalControlNotLinked { goal, control }),
            _ => Ok(()),
        }
    }

    fn check_goal_party(&self, goal: GoalId, party: PartyId, link: bool) -> Result<(), ModelError> {
        let (_, g) = self.live_goal(goal)?;
        self.party_index(party)?;
        match (link, self.find_goal_party(g, party).is_ok()) {
            (true, true) => Err(ModelError::DuplicateGoalParty { goal, party }),
            (true, false) if g.responsible.len() >= MAX_RESPONSIBLE => {
                Err(ModelError::TooManyGoalResponsible {
                    max: MAX_RESPONSIBLE,
                })
            }
            (false, false) => Err(ModelError::GoalPartyNotLinked { goal, party }),
            _ => Ok(()),
        }
    }

    fn check_claim_party(
        &self,
        claim: ClaimId,
        party: PartyId,
        link: bool,
    ) -> Result<(), ModelError> {
        let c = self.live_claim_ref(claim)?;
        self.party_index(party)?;
        match (link, self.find_claim_party(c, party).is_ok()) {
            (true, true) => Err(ModelError::DuplicateClaimParty { claim, party }),
            (true, false) if c.responsible.len() >= MAX_RESPONSIBLE => {
                Err(ModelError::TooManyClaimResponsible {
                    max: MAX_RESPONSIBLE,
                })
            }
            (false, false) => Err(ModelError::ClaimPartyNotLinked { claim, party }),
            _ => Ok(()),
        }
    }

    fn check_remove_goal(&self, id: GoalId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::Goal(id), EntityKind::CybersecurityGoal)?;
        let (idx, g) = self.live_goal(id)?;
        let threats = self.refs.goals.get(idx.index()).copied().unwrap_or(0);
        if threats > 0 {
            return Err(ModelError::CybersecurityGoalInUse { goal: id, threats });
        }
        if !g.responsible.is_empty() {
            return Err(ModelError::CybersecurityGoalHasParties {
                goal: id,
                parties: g.responsible.len(),
            });
        }
        Ok(())
    }

    fn check_add_claim(
        &self,
        id: ClaimId,
        name: &str,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.claims.idx(&id).is_some() {
            return Err(ModelError::DuplicateCybersecurityClaim(id));
        }
        check_name(EntityKind::CybersecurityClaim, name)?;
        check_provenance(provenance)?;
        if self.claims.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::CybersecurityClaim,
            });
        }
        Ok(())
    }

    fn check_remove_claim(&self, id: ClaimId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::Claim(id), EntityKind::CybersecurityClaim)?;
        let idx = self.live_claim(id)?;
        let threats = self.refs.claims.get(idx.index()).copied().unwrap_or(0);
        if threats > 0 {
            return Err(ModelError::CybersecurityClaimInUse { claim: id, threats });
        }
        let parties = self.live_claim_ref(id)?.responsible.len();
        if parties > 0 {
            return Err(ModelError::CybersecurityClaimHasParties { claim: id, parties });
        }
        Ok(())
    }

    fn check_add_party(
        &self,
        id: PartyId,
        name: &str,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.parties.idx(&id).is_some() {
            return Err(ModelError::DuplicatePartyId(id));
        }
        check_party_name(name)?;
        check_provenance(provenance)?;
        if self.parties.len() >= MAX_PARTIES || self.parties.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::TooManyParties { max: MAX_PARTIES });
        }
        Ok(())
    }

    fn check_remove_party(&self, id: PartyId) -> Result<(), ModelError> {
        let idx = self.party_index(id)?;
        let goals = self
            .goals
            .iter()
            .filter(|(_, _, g)| g.responsible.contains(&idx))
            .count();
        let claims = self
            .claims
            .iter()
            .filter(|(_, _, c)| c.responsible.contains(&idx))
            .count();
        if goals > 0 || claims > 0 {
            return Err(ModelError::PartyInUse {
                party: id,
                goals,
                claims,
            });
        }
        Ok(())
    }

    fn check_set_threat_goal(
        &self,
        threat: ThreatId,
        goal: Option<GoalId>,
    ) -> Result<(), ModelError> {
        self.live_threat(threat)?;
        if let Some(goal) = goal {
            self.live_goal(goal)?;
        }
        Ok(())
    }

    fn check_set_threat_claim(
        &self,
        threat: ThreatId,
        claim: Option<ClaimId>,
    ) -> Result<(), ModelError> {
        self.live_threat(threat)?;
        if let Some(claim) = claim {
            self.live_claim(claim)?;
        }
        Ok(())
    }

    /// Checks whether setting `parent` on `id` (with `child_idx: Some(idx)` for
    /// an existing component or `None` for a new component) is cycle-free and
    /// keeps the containment tree within [`MAX_COMPONENT_DEPTH`].
    fn check_component_parent_chain(
        &self,
        id: ComponentId,
        child_idx: Option<ComponentIdx>,
        parent: Option<ComponentId>,
    ) -> Result<(), ModelError> {
        let Some(parent_id) = parent else {
            return Ok(());
        };
        let (parent_idx, _) = self.live_component(parent_id)?;
        if child_idx == Some(parent_idx) {
            return Err(ModelError::ComponentCycle {
                component: id,
                parent: parent_id,
            });
        }
        let mut ancestor_hops = 1usize;
        let mut cur = Some(parent_idx);
        while let Some(c_idx) = cur {
            if child_idx == Some(c_idx) {
                return Err(ModelError::ComponentCycle {
                    component: id,
                    parent: parent_id,
                });
            }
            let Some(c) = self.components.get(c_idx) else {
                break;
            };
            cur = c.parent;
            if cur.is_some() {
                ancestor_hops += 1;
            }
            if ancestor_hops > MAX_COMPONENT_DEPTH {
                return Err(ModelError::ComponentDepthExceeded {
                    component: id,
                    max: MAX_COMPONENT_DEPTH,
                });
            }
        }
        if let Some(c_idx) = child_idx {
            let sub_hops = self.max_descendant_hops(c_idx);
            if ancestor_hops + sub_hops > MAX_COMPONENT_DEPTH {
                return Err(ModelError::ComponentDepthExceeded {
                    component: id,
                    max: MAX_COMPONENT_DEPTH,
                });
            }
        }
        Ok(())
    }

    /// Maximum parent-chain hops below `root_idx`. Time O(C) when `root_idx`
    /// has children, O(1) when it has none.
    fn max_descendant_hops(&self, root_idx: ComponentIdx) -> usize {
        if self
            .refs
            .component_children
            .get(root_idx.index())
            .copied()
            .unwrap_or(0)
            == 0
        {
            return 0;
        }
        let mut max_hops = 0usize;
        for (idx, _, _) in self.components.iter() {
            let mut hops = 0usize;
            let mut cur = self.components.get(idx).and_then(|c| c.parent);
            while let Some(p_idx) = cur {
                hops += 1;
                if p_idx == root_idx {
                    max_hops = max_hops.max(hops);
                    break;
                }
                if hops > MAX_COMPONENT_DEPTH {
                    break;
                }
                cur = self.components.get(p_idx).and_then(|c| c.parent);
            }
        }
        max_hops
    }

    fn check_add_component(
        &self,
        id: ComponentId,
        name: &str,
        parent: Option<ComponentId>,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.components.idx(&id).is_some() {
            return Err(ModelError::DuplicateComponent(id));
        }
        check_name(EntityKind::Component, name)?;
        self.check_component_parent_chain(id, None, parent)?;
        check_provenance(provenance)?;
        if self.components.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Component,
            });
        }
        Ok(())
    }

    fn check_set_component_parent(
        &self,
        id: ComponentId,
        parent: Option<ComponentId>,
    ) -> Result<(), ModelError> {
        let (idx, c) = self.live_component(id)?;
        if parent.is_some() && c.system.is_some() {
            return Err(ModelError::ComponentHasSystem { component: id });
        }
        self.check_component_parent_chain(id, Some(idx), parent)
    }

    fn check_set_component_system(
        &self,
        id: ComponentId,
        system: Option<ElementId>,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        let (_, c) = self.live_component(id)?;
        if let Some(sys) = system {
            if c.parent.is_some() {
                return Err(ModelError::ChildComponentSystem { component: id });
            }
            let e = self
                .elements
                .get(sys)
                .ok_or(ModelError::UnknownElement(sys))?;
            if e.kind() != ElementKind::System {
                return Err(ModelError::NotASystem { element: sys });
            }
        }
        check_provenance(provenance)
    }

    fn check_remove_component(&self, id: ComponentId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::Component(id), EntityKind::Component)?;
        let (idx, c) = self.live_component(id)?;
        if c.system.is_some() {
            return Err(ModelError::ComponentHasSystem { component: id });
        }
        let pos = idx.index();
        let children = self.refs.component_children.get(pos).copied().unwrap_or(0);
        if children > 0 {
            return Err(ModelError::ComponentHasChildren {
                component: id,
                children,
            });
        }
        let interfaces = self
            .refs
            .component_interfaces
            .get(pos)
            .copied()
            .unwrap_or(0);
        if interfaces > 0 {
            return Err(ModelError::ComponentHasInterfaces {
                component: id,
                interfaces,
            });
        }
        let flows = self.refs.component_flows.get(pos).copied().unwrap_or(0);
        if flows > 0 {
            return Err(ModelError::ComponentInDataFlow {
                component: id,
                flows,
            });
        }
        let boundaries = self
            .refs
            .component_boundaries
            .get(pos)
            .copied()
            .unwrap_or(0);
        if boundaries > 0 {
            return Err(ModelError::ComponentInTrustBoundary {
                component: id,
                boundaries,
            });
        }
        let assets = self.refs.component_assets.get(pos).copied().unwrap_or(0);
        if assets > 0 {
            return Err(ModelError::ComponentHasAssets {
                component: id,
                assets,
            });
        }
        let steps = self.count_steps_via(ItemIdx::Component(idx));
        if steps > 0 {
            return Err(ModelError::ComponentHasSteps {
                component: id,
                steps,
            });
        }
        let controls = self.refs.component_controls.get(pos).copied().unwrap_or(0);
        if controls > 0 {
            return Err(ModelError::ComponentHasControls {
                component: id,
                controls,
            });
        }
        Ok(())
    }

    fn check_add_interface(
        &self,
        id: InterfaceId,
        name: &str,
        component: ComponentId,
        kind: InterfaceKind,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.interfaces.idx(&id).is_some() {
            return Err(ModelError::DuplicateInterface(id));
        }
        check_name(EntityKind::Interface, name)?;
        self.live_component(component)?;
        self.methodology.check_interface_kind(kind)?;
        check_provenance(provenance)?;
        if self.interfaces.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Interface,
            });
        }
        Ok(())
    }

    fn check_remove_interface(&self, id: InterfaceId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::Interface(id), EntityKind::Interface)?;
        let (idx, _) = self.live_interface(id)?;
        let flows = self.refs.interfaces.get(idx.index()).copied().unwrap_or(0);
        if flows > 0 {
            return Err(ModelError::InterfaceInUse {
                interface: id,
                flows,
            });
        }
        let steps = self.count_steps_via(ItemIdx::Interface(idx));
        if steps > 0 {
            return Err(ModelError::InterfaceHasSteps {
                interface: id,
                steps,
            });
        }
        let controls = self
            .refs
            .interface_controls
            .get(idx.index())
            .copied()
            .unwrap_or(0);
        if controls > 0 {
            return Err(ModelError::InterfaceHasControls {
                interface: id,
                controls,
            });
        }
        Ok(())
    }

    fn check_add_data_flow(
        &self,
        id: DataFlowId,
        name: &str,
        from: ComponentId,
        to: ComponentId,
        interface: Option<InterfaceId>,
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.data_flows.idx(&id).is_some() {
            return Err(ModelError::DuplicateDataFlow(id));
        }
        check_name(EntityKind::DataFlow, name)?;
        let (from_idx, _) = self.live_component(from)?;
        let (to_idx, _) = self.live_component(to)?;
        if let Some(iface_id) = interface {
            let (_, iface) = self.live_interface(iface_id)?;
            if iface.component != from_idx && iface.component != to_idx {
                return Err(ModelError::InterfaceNotOnEndpoint {
                    flow: id,
                    interface: iface_id,
                });
            }
        }
        check_provenance(provenance)?;
        if self.data_flows.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::DataFlow,
            });
        }
        Ok(())
    }

    fn check_set_data_flow_interface(
        &self,
        id: DataFlowId,
        interface: Option<InterfaceId>,
    ) -> Result<(), ModelError> {
        let (_, df) = self.live_data_flow(id)?;
        if let Some(iface_id) = interface {
            let (_, iface) = self.live_interface(iface_id)?;
            if iface.component != df.from && iface.component != df.to {
                return Err(ModelError::InterfaceNotOnEndpoint {
                    flow: id,
                    interface: iface_id,
                });
            }
        }
        Ok(())
    }

    fn check_remove_data_flow(&self, id: DataFlowId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::DataFlow(id), EntityKind::DataFlow)?;
        self.live_data_flow(id)?;
        Ok(())
    }

    fn check_add_trust_boundary(
        &self,
        id: TrustBoundaryId,
        name: &str,
        components: &[ComponentId],
        provenance: &Provenance,
    ) -> Result<(), ModelError> {
        if self.trust_boundaries.idx(&id).is_some() {
            return Err(ModelError::DuplicateTrustBoundary(id));
        }
        check_name(EntityKind::TrustBoundary, name)?;
        if components.len() > MAX_TRUST_BOUNDARY_MEMBERS {
            return Err(ModelError::TooManyBoundaryMembers {
                boundary: id,
                max: MAX_TRUST_BOUNDARY_MEMBERS,
            });
        }
        for &c in components {
            self.live_component(c)?;
        }
        let mut ids = components.to_vec();
        ids.sort_unstable();
        if let Some(&component) = ids
            .windows(2)
            .find(|w| w.first() == w.get(1))
            .and_then(|w| w.first())
        {
            return Err(ModelError::DuplicateBoundaryComponent {
                boundary: id,
                component,
            });
        }
        check_provenance(provenance)?;
        if self.trust_boundaries.next_idx(self.slot_limit).is_none() {
            return Err(ModelError::ProjectCapacityExceeded {
                kind: EntityKind::TrustBoundary,
            });
        }
        Ok(())
    }

    fn check_boundary_component(
        &self,
        boundary: TrustBoundaryId,
        component: ComponentId,
        link: bool,
    ) -> Result<(), ModelError> {
        let (_, tb) = self.live_trust_boundary(boundary)?;
        self.live_component(component)?;
        match (link, self.find_boundary_component(tb, component).is_ok()) {
            (true, true) => Err(ModelError::DuplicateBoundaryComponent {
                boundary,
                component,
            }),
            (true, false) if tb.components.len() >= MAX_TRUST_BOUNDARY_MEMBERS => {
                Err(ModelError::TooManyBoundaryMembers {
                    boundary,
                    max: MAX_TRUST_BOUNDARY_MEMBERS,
                })
            }
            (false, false) => Err(ModelError::BoundaryComponentNotLinked {
                boundary,
                component,
            }),
            _ => Ok(()),
        }
    }

    fn check_remove_trust_boundary(&self, id: TrustBoundaryId) -> Result<(), ModelError> {
        self.require_no_description(ElementRef::TrustBoundary(id), EntityKind::TrustBoundary)?;
        self.live_trust_boundary(id)?;
        Ok(())
    }

    fn check_set_asset_component(
        &self,
        asset: AssetId,
        component: Option<ComponentId>,
    ) -> Result<(), ModelError> {
        self.live_asset(asset)?;
        if let Some(component) = component {
            self.live_component(component)?;
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
            component: None,
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
            impact_rationale: Arc::from(""),
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
            damage_rationales: Vec::new(),
            direct_feasibility: None,
            attack_tree: None,
            treatment: None,
            goal: None,
            claim: None,
            scales: None,
            scaling_reason: Arc::from(""),
            feasibility_rationale: Arc::from(""),
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
        let key = self.damage_key(d);
        let overlay = t
            .damage_rationales
            .binary_search_by_key(&key, |(d, _)| self.damage_key(*d));
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
                if let Ok(i) = overlay {
                    t.damage_rationales.remove(i);
                }
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
        let step_factors = vec![Box::default(); tree.len()];
        let step_mitigations = vec![Box::default(); tree.len()];
        let entry = TreeEntry {
            tree,
            step_feasibility: slots,
            step_factors,
            step_mitigations,
            step_links: None,
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

    fn commit_set_step_via(
        &mut self,
        tree: TreeId,
        node: NodeId,
        via: Option<ItemRef>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let new = via.map(|item| self.item_idx(item)).transpose()?;
        let entry = self.tree_mut(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let old = entry.set_link(node_idx.index(), new, |l| &mut l.via);
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::SetStepVia {
            tree,
            node,
            via: self.item_ref(old),
            provenance,
        })
    }

    fn commit_set_step_attack_type(
        &mut self,
        tree: TreeId,
        node: NodeId,
        attack_type: Option<AttackType>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let entry = self.tree_mut(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let attack_type = entry.set_link(node_idx.index(), attack_type, |l| &mut l.attack_type);
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::SetStepAttackType {
            tree,
            node,
            attack_type,
            provenance,
        })
    }

    fn commit_set_step_factor(
        &mut self,
        tree: TreeId,
        node: NodeId,
        factor: FeasibilityFactor,
        level: Option<FactorLevel>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let factor_count = self.methodology.feasibility_factor_count();
        let entry = self.tree_mut(tree)?;
        let node_idx = entry
            .tree
            .idx_of(&node)
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let row = entry
            .step_factors
            .get_mut(node_idx.index())
            .ok_or(ModelError::UnknownTreeNode { tree, node })?;
        let old = if let Some(new_l) = level {
            if row.is_empty() {
                *row = vec![None; factor_count].into_boxed_slice();
            }
            let slot = row
                .get_mut(factor.index())
                .ok_or(ModelError::UnknownFeasibilityFactor(factor))?;
            slot.replace(new_l)
        } else {
            let old = row.get_mut(factor.index()).and_then(Option::take);
            if row.iter().all(Option::is_none) {
                *row = Box::default();
            }
            old
        };
        let provenance = std::mem::replace(&mut entry.provenance, provenance);
        Ok(Op::SetStepFactor {
            tree,
            node,
            factor,
            level: old,
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
        let control = SecurityControl {
            name,
            targets: Vec::new(),
            rules: Vec::new(),
            kind: None,
            provenance,
        };
        self.controls.push(id, control, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::SecurityControl,
            },
        )?;
        self.refs.controls.push(0);
        self.refs.control_goals.push(0);
        Ok(Op::RemoveSecurityControl { id })
    }

    fn bump_item_controls(&mut self, item: ItemIdx, up: bool) {
        match item {
            ItemIdx::Component(c) => {
                bump(&mut self.refs.component_controls, Some(c.index()), up);
            }
            ItemIdx::Interface(i) => {
                bump(&mut self.refs.interface_controls, Some(i.index()), up);
            }
        }
    }

    fn commit_control_target(
        &mut self,
        control: ControlId,
        target: ItemRef,
        provenance: Provenance,
        apply: bool,
    ) -> Result<Op, ModelError> {
        let idx = self.control_index(control)?;
        let t_idx = self.item_idx(target)?;
        let c = self
            .controls
            .get(idx)
            .ok_or(ModelError::UnknownSecurityControl(control))?;
        let found = self.find_control_target(c, target);
        let c = self
            .controls
            .get_mut(idx)
            .ok_or(ModelError::UnknownSecurityControl(control))?;
        match (apply, found) {
            (true, Err(pos)) => {
                c.targets.insert(pos, t_idx);
                let provenance = std::mem::replace(&mut c.provenance, provenance);
                self.bump_item_controls(t_idx, true);
                Ok(Op::UnapplyControl {
                    control,
                    target,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                c.targets.remove(pos);
                let provenance = std::mem::replace(&mut c.provenance, provenance);
                self.bump_item_controls(t_idx, false);
                Ok(Op::ApplyControl {
                    control,
                    target,
                    provenance,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateControlTarget { control, target }),
            (false, Err(_)) => Err(ModelError::ControlTargetNotLinked { control, target }),
        }
    }

    fn commit_set_control_rule(
        &mut self,
        control: ControlId,
        attack_type: AttackType,
        ceiling: Option<FeasibilityLevel>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let c = self.control_mut(control)?;
        let found = c
            .rules
            .binary_search_by_key(&attack_type, |r| r.attack_type);
        let old = match (ceiling, found) {
            (Some(new_c), Ok(pos)) => {
                let old_c = std::mem::replace(&mut c.rules[pos].ceiling, new_c);
                Some(old_c)
            }
            (Some(new_c), Err(pos)) => {
                c.rules.insert(
                    pos,
                    ControlRule {
                        attack_type,
                        ceiling: new_c,
                    },
                );
                None
            }
            (None, Ok(pos)) => Some(c.rules.remove(pos).ceiling),
            (None, Err(_)) => None,
        };
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        Ok(Op::SetControlRule {
            control,
            attack_type,
            ceiling: old,
            provenance,
        })
    }

    fn commit_set_control_kind(
        &mut self,
        control: ControlId,
        kind: Option<ControlKind>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let c = self.control_mut(control)?;
        let old = std::mem::replace(&mut c.kind, kind);
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        Ok(Op::SetControlKind {
            control,
            kind: old,
            provenance,
        })
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

    fn commit_add_goal(
        &mut self,
        id: GoalId,
        name: Arc<str>,
        mut controls: Vec<ControlId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        controls.sort_unstable();
        let controls = controls
            .into_iter()
            .map(|c| self.control_index(c))
            .collect::<Result<Vec<_>, _>>()?;
        let g = CybersecurityGoal {
            name,
            controls,
            responsible: Vec::new(),
            provenance,
        };
        let idx =
            self.goals
                .push(id, g, self.slot_limit)
                .ok_or(ModelError::ProjectCapacityExceeded {
                    kind: EntityKind::CybersecurityGoal,
                })?;
        self.refs.goals.push(0);
        if let Some(g) = self.goals.get(idx) {
            for &c in &g.controls {
                bump(&mut self.refs.control_goals, Some(c.index()), true);
            }
        }
        Ok(Op::RemoveCybersecurityGoal { id })
    }

    fn commit_rename_goal(
        &mut self,
        id: GoalId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let g = self.goal_mut(id)?;
        let name = std::mem::replace(&mut g.name, name);
        let provenance = std::mem::replace(&mut g.provenance, provenance);
        Ok(Op::RenameCybersecurityGoal {
            id,
            name,
            provenance,
        })
    }

    fn commit_goal_control(
        &mut self,
        goal: GoalId,
        control: ControlId,
        provenance: Provenance,
        link: bool,
    ) -> Result<Op, ModelError> {
        let idx = self.goal_index(goal)?;
        let c = self.control_index(control)?;
        let g = self
            .goals
            .get(idx)
            .ok_or(ModelError::UnknownCybersecurityGoal(goal))?;
        let found = self.find_goal_control(g, control);
        let g = self
            .goals
            .get_mut(idx)
            .ok_or(ModelError::UnknownCybersecurityGoal(goal))?;
        match (link, found) {
            (true, Err(pos)) => {
                g.controls.insert(pos, c);
                let provenance = std::mem::replace(&mut g.provenance, provenance);
                bump(&mut self.refs.control_goals, Some(c.index()), true);
                Ok(Op::UnlinkGoalControl {
                    goal,
                    control,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                g.controls.remove(pos);
                let provenance = std::mem::replace(&mut g.provenance, provenance);
                bump(&mut self.refs.control_goals, Some(c.index()), false);
                Ok(Op::LinkGoalControl {
                    goal,
                    control,
                    provenance,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateGoalControl { goal, control }),
            (false, Err(_)) => Err(ModelError::GoalControlNotLinked { goal, control }),
        }
    }

    fn commit_goal_party(
        &mut self,
        goal: GoalId,
        party: PartyId,
        provenance: Provenance,
        link: bool,
    ) -> Result<Op, ModelError> {
        let p = self.party_index(party)?;
        let found = self.find_goal_party(self.live_goal(goal)?.1, party);
        let g = self.goal_mut(goal)?;
        match (link, found) {
            (true, Err(pos)) => {
                g.responsible.insert(pos, p);
                let provenance = std::mem::replace(&mut g.provenance, provenance);
                Ok(Op::UnlinkGoalParty {
                    goal,
                    party,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                g.responsible.remove(pos);
                let provenance = std::mem::replace(&mut g.provenance, provenance);
                Ok(Op::LinkGoalParty {
                    goal,
                    party,
                    provenance,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateGoalParty { goal, party }),
            (false, Err(_)) => Err(ModelError::GoalPartyNotLinked { goal, party }),
        }
    }

    fn commit_remove_goal(&mut self, id: GoalId) -> Result<Op, ModelError> {
        let idx = self.goal_index(id)?;
        let (_, g) = self.live_goal(id)?;
        let controls = g
            .controls
            .iter()
            .map(|&c| {
                self.control_key(c)
                    .ok_or(ModelError::UnknownCybersecurityGoal(id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let g = self
            .goals
            .remove(idx)
            .ok_or(ModelError::UnknownCybersecurityGoal(id))?;
        for &c in &g.controls {
            bump(&mut self.refs.control_goals, Some(c.index()), false);
        }
        Ok(Op::AddCybersecurityGoal {
            id,
            name: g.name,
            controls,
            provenance: g.provenance,
        })
    }

    fn commit_add_claim(
        &mut self,
        id: ClaimId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let claim = CybersecurityClaim {
            name,
            responsible: Vec::new(),
            provenance,
        };
        self.claims.push(id, claim, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::CybersecurityClaim,
            },
        )?;
        self.refs.claims.push(0);
        Ok(Op::RemoveCybersecurityClaim { id })
    }

    fn commit_rename_claim(
        &mut self,
        id: ClaimId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let c = self.claim_mut(id)?;
        let name = std::mem::replace(&mut c.name, name);
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        Ok(Op::RenameCybersecurityClaim {
            id,
            name,
            provenance,
        })
    }

    fn commit_claim_party(
        &mut self,
        claim: ClaimId,
        party: PartyId,
        provenance: Provenance,
        link: bool,
    ) -> Result<Op, ModelError> {
        let p = self.party_index(party)?;
        let found = self.find_claim_party(self.live_claim_ref(claim)?, party);
        let c = self.claim_mut(claim)?;
        match (link, found) {
            (true, Err(pos)) => {
                c.responsible.insert(pos, p);
                let provenance = std::mem::replace(&mut c.provenance, provenance);
                Ok(Op::UnlinkClaimParty {
                    claim,
                    party,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                c.responsible.remove(pos);
                let provenance = std::mem::replace(&mut c.provenance, provenance);
                Ok(Op::LinkClaimParty {
                    claim,
                    party,
                    provenance,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateClaimParty { claim, party }),
            (false, Err(_)) => Err(ModelError::ClaimPartyNotLinked { claim, party }),
        }
    }

    fn commit_remove_claim(&mut self, id: ClaimId) -> Result<Op, ModelError> {
        let idx = self.claim_index(id)?;
        let c = self
            .claims
            .remove(idx)
            .ok_or(ModelError::UnknownCybersecurityClaim(id))?;
        Ok(Op::AddCybersecurityClaim {
            id,
            name: c.name,
            provenance: c.provenance,
        })
    }

    fn commit_add_party(
        &mut self,
        id: PartyId,
        name: Arc<str>,
        kind: PartyKind,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let party = Party {
            name,
            kind,
            provenance,
        };
        self.parties
            .push(id, party, self.slot_limit)
            .ok_or(ModelError::TooManyParties { max: MAX_PARTIES })?;
        Ok(Op::RemoveParty { id })
    }

    fn commit_rename_party(
        &mut self,
        id: PartyId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let p = self.party_mut(id)?;
        let name = std::mem::replace(&mut p.name, name);
        let provenance = std::mem::replace(&mut p.provenance, provenance);
        Ok(Op::RenameParty {
            id,
            name,
            provenance,
        })
    }

    fn commit_set_party_kind(
        &mut self,
        id: PartyId,
        kind: PartyKind,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let p = self.party_mut(id)?;
        let kind = std::mem::replace(&mut p.kind, kind);
        let provenance = std::mem::replace(&mut p.provenance, provenance);
        Ok(Op::SetPartyKind {
            id,
            kind,
            provenance,
        })
    }

    fn commit_remove_party(&mut self, id: PartyId) -> Result<Op, ModelError> {
        let idx = self.party_index(id)?;
        let p = self
            .parties
            .remove(idx)
            .ok_or(ModelError::UnknownParty(id))?;
        Ok(Op::AddParty {
            id,
            name: p.name,
            kind: p.kind,
            provenance: p.provenance,
        })
    }

    fn commit_set_threat_goal(
        &mut self,
        threat: ThreatId,
        goal: Option<GoalId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(threat)?;
        let new_idx = goal.map(|g| self.goal_index(g)).transpose()?;
        let old_idx = self
            .threats
            .get(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?
            .goal;
        let old_goal = old_idx
            .map(|i| {
                self.goals
                    .id(i)
                    .ok_or(ModelError::UnknownThreatScenario(threat))
            })
            .transpose()?;
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?;
        t.goal = new_idx;
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        if let Some(old) = old_idx {
            bump(&mut self.refs.goals, Some(old.index()), false);
        }
        if let Some(new) = new_idx {
            bump(&mut self.refs.goals, Some(new.index()), true);
        }
        Ok(Op::SetThreatGoal {
            threat,
            goal: old_goal,
            provenance,
        })
    }

    fn commit_set_threat_claim(
        &mut self,
        threat: ThreatId,
        claim: Option<ClaimId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(threat)?;
        let new_idx = claim.map(|c| self.claim_index(c)).transpose()?;
        let old_idx = self
            .threats
            .get(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?
            .claim;
        let old_claim = old_idx
            .map(|i| {
                self.claims
                    .id(i)
                    .ok_or(ModelError::UnknownThreatScenario(threat))
            })
            .transpose()?;
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?;
        t.claim = new_idx;
        let provenance = std::mem::replace(&mut t.provenance, provenance);
        if let Some(old) = old_idx {
            bump(&mut self.refs.claims, Some(old.index()), false);
        }
        if let Some(new) = new_idx {
            bump(&mut self.refs.claims, Some(new.index()), true);
        }
        Ok(Op::SetThreatClaim {
            threat,
            claim: old_claim,
            provenance,
        })
    }

    fn commit_set_threat_scaling(
        &mut self,
        threat: ThreatId,
        scales: Option<bool>,
        reason: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.threat_index(threat)?;
        let t = self
            .threats
            .get_mut(idx)
            .ok_or(ModelError::UnknownThreatScenario(threat))?;
        Ok(Op::SetThreatScaling {
            threat,
            scales: std::mem::replace(&mut t.scales, scales),
            reason: std::mem::replace(&mut t.scaling_reason, reason),
            provenance: std::mem::replace(&mut t.provenance, provenance),
        })
    }

    fn commit_add_component(
        &mut self,
        id: ComponentId,
        name: Arc<str>,
        parent: Option<ComponentId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let parent_idx = parent.map(|p| self.component_index(p)).transpose()?;
        let comp = Component {
            name,
            parent: parent_idx,
            system: None,
            provenance,
        };
        self.components.push(id, comp, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Component,
            },
        )?;
        self.refs.component_children.push(0);
        self.refs.component_interfaces.push(0);
        self.refs.component_flows.push(0);
        self.refs.component_boundaries.push(0);
        self.refs.component_assets.push(0);
        self.refs.component_controls.push(0);
        if let Some(p_idx) = parent_idx {
            bump(&mut self.refs.component_children, Some(p_idx.index()), true);
        }
        Ok(Op::RemoveComponent { id })
    }

    fn commit_rename_component(
        &mut self,
        id: ComponentId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let c = self.component_mut(id)?;
        let name = std::mem::replace(&mut c.name, name);
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        Ok(Op::RenameComponent {
            id,
            name,
            provenance,
        })
    }

    fn commit_set_component_parent(
        &mut self,
        id: ComponentId,
        parent: Option<ComponentId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.component_index(id)?;
        let new_idx = parent.map(|p| self.component_index(p)).transpose()?;
        let old_idx = self
            .components
            .get(idx)
            .ok_or(ModelError::UnknownComponent(id))?
            .parent;
        let old_parent = old_idx
            .map(|i| {
                self.components
                    .id(i)
                    .ok_or(ModelError::UnknownComponent(id))
            })
            .transpose()?;
        let c = self
            .components
            .get_mut(idx)
            .ok_or(ModelError::UnknownComponent(id))?;
        c.parent = new_idx;
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        if let Some(old) = old_idx {
            bump(&mut self.refs.component_children, Some(old.index()), false);
        }
        if let Some(new) = new_idx {
            bump(&mut self.refs.component_children, Some(new.index()), true);
        }
        Ok(Op::SetComponentParent {
            id,
            parent: old_parent,
            provenance,
        })
    }

    fn commit_set_component_system(
        &mut self,
        id: ComponentId,
        system: Option<ElementId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let c = self.component_mut(id)?;
        let old = std::mem::replace(&mut c.system, system);
        let provenance = std::mem::replace(&mut c.provenance, provenance);
        Ok(Op::SetComponentSystem {
            id,
            system: old,
            provenance,
        })
    }

    fn commit_remove_component(&mut self, id: ComponentId) -> Result<Op, ModelError> {
        let idx = self.component_index(id)?;
        let (_, c) = self.live_component(id)?;
        let parent = c
            .parent
            .map(|p| {
                self.components
                    .id(p)
                    .ok_or(ModelError::UnknownComponent(id))
            })
            .transpose()?;
        let c = self
            .components
            .remove(idx)
            .ok_or(ModelError::UnknownComponent(id))?;
        if let Some(p_idx) = c.parent {
            bump(
                &mut self.refs.component_children,
                Some(p_idx.index()),
                false,
            );
        }
        Ok(Op::AddComponent {
            id,
            name: c.name,
            parent,
            provenance: c.provenance,
        })
    }

    fn commit_add_interface(
        &mut self,
        id: InterfaceId,
        name: Arc<str>,
        component: ComponentId,
        kind: InterfaceKind,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let comp_idx = self.component_index(component)?;
        let iface = Interface {
            name,
            component: comp_idx,
            kind,
            provenance,
        };
        self.interfaces.push(id, iface, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Interface,
            },
        )?;
        self.refs.interfaces.push(0);
        self.refs.interface_controls.push(0);
        bump(
            &mut self.refs.component_interfaces,
            Some(comp_idx.index()),
            true,
        );
        Ok(Op::RemoveInterface { id })
    }

    fn commit_rename_interface(
        &mut self,
        id: InterfaceId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let i = self.interface_mut(id)?;
        let name = std::mem::replace(&mut i.name, name);
        let provenance = std::mem::replace(&mut i.provenance, provenance);
        Ok(Op::RenameInterface {
            id,
            name,
            provenance,
        })
    }

    fn commit_set_interface_kind(
        &mut self,
        id: InterfaceId,
        kind: InterfaceKind,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let i = self.interface_mut(id)?;
        let kind = std::mem::replace(&mut i.kind, kind);
        let provenance = std::mem::replace(&mut i.provenance, provenance);
        Ok(Op::SetInterfaceKind {
            id,
            kind,
            provenance,
        })
    }

    fn commit_remove_interface(&mut self, id: InterfaceId) -> Result<Op, ModelError> {
        let idx = self.interface_index(id)?;
        let (_, iface) = self.live_interface(id)?;
        let component = self
            .components
            .id(iface.component)
            .ok_or(ModelError::UnknownInterface(id))?;
        let iface = self
            .interfaces
            .remove(idx)
            .ok_or(ModelError::UnknownInterface(id))?;
        bump(
            &mut self.refs.component_interfaces,
            Some(iface.component.index()),
            false,
        );
        Ok(Op::AddInterface {
            id,
            name: iface.name,
            component,
            kind: iface.kind,
            provenance: iface.provenance,
        })
    }

    fn commit_add_data_flow(
        &mut self,
        id: DataFlowId,
        name: Arc<str>,
        from: ComponentId,
        to: ComponentId,
        interface: Option<InterfaceId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let from_idx = self.component_index(from)?;
        let to_idx = self.component_index(to)?;
        let iface_idx = interface.map(|i| self.interface_index(i)).transpose()?;
        let df = DataFlow {
            name,
            from: from_idx,
            to: to_idx,
            interface: iface_idx,
            provenance,
        };
        self.data_flows.push(id, df, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::DataFlow,
            },
        )?;
        bump(&mut self.refs.component_flows, Some(from_idx.index()), true);
        if to_idx != from_idx {
            bump(&mut self.refs.component_flows, Some(to_idx.index()), true);
        }
        if let Some(i_idx) = iface_idx {
            bump(&mut self.refs.interfaces, Some(i_idx.index()), true);
        }
        Ok(Op::RemoveDataFlow { id })
    }

    fn commit_rename_data_flow(
        &mut self,
        id: DataFlowId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let df = self.data_flow_mut(id)?;
        let name = std::mem::replace(&mut df.name, name);
        let provenance = std::mem::replace(&mut df.provenance, provenance);
        Ok(Op::RenameDataFlow {
            id,
            name,
            provenance,
        })
    }

    fn commit_set_data_flow_interface(
        &mut self,
        id: DataFlowId,
        interface: Option<InterfaceId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.data_flow_index(id)?;
        let new_idx = interface.map(|i| self.interface_index(i)).transpose()?;
        let old_idx = self
            .data_flows
            .get(idx)
            .ok_or(ModelError::UnknownDataFlow(id))?
            .interface;
        let old_interface = old_idx
            .map(|i| self.interfaces.id(i).ok_or(ModelError::UnknownDataFlow(id)))
            .transpose()?;
        let df = self
            .data_flows
            .get_mut(idx)
            .ok_or(ModelError::UnknownDataFlow(id))?;
        df.interface = new_idx;
        let provenance = std::mem::replace(&mut df.provenance, provenance);
        if let Some(old) = old_idx {
            bump(&mut self.refs.interfaces, Some(old.index()), false);
        }
        if let Some(new) = new_idx {
            bump(&mut self.refs.interfaces, Some(new.index()), true);
        }
        Ok(Op::SetDataFlowInterface {
            id,
            interface: old_interface,
            provenance,
        })
    }

    fn commit_remove_data_flow(&mut self, id: DataFlowId) -> Result<Op, ModelError> {
        let idx = self.data_flow_index(id)?;
        let (_, df) = self.live_data_flow(id)?;
        let from = self
            .components
            .id(df.from)
            .ok_or(ModelError::UnknownDataFlow(id))?;
        let to = self
            .components
            .id(df.to)
            .ok_or(ModelError::UnknownDataFlow(id))?;
        let interface = df
            .interface
            .map(|i| self.interfaces.id(i).ok_or(ModelError::UnknownDataFlow(id)))
            .transpose()?;
        let df = self
            .data_flows
            .remove(idx)
            .ok_or(ModelError::UnknownDataFlow(id))?;
        bump(&mut self.refs.component_flows, Some(df.from.index()), false);
        if df.to != df.from {
            bump(&mut self.refs.component_flows, Some(df.to.index()), false);
        }
        if let Some(i_idx) = df.interface {
            bump(&mut self.refs.interfaces, Some(i_idx.index()), false);
        }
        Ok(Op::AddDataFlow {
            id,
            name: df.name,
            from,
            to,
            interface,
            provenance: df.provenance,
        })
    }

    fn commit_add_trust_boundary(
        &mut self,
        id: TrustBoundaryId,
        name: Arc<str>,
        mut components: Vec<ComponentId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        components.sort_unstable();
        let components = components
            .into_iter()
            .map(|c| self.component_index(c))
            .collect::<Result<Vec<_>, _>>()?;
        let tb = TrustBoundary {
            name,
            components,
            provenance,
        };
        let idx = self.trust_boundaries.push(id, tb, self.slot_limit).ok_or(
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::TrustBoundary,
            },
        )?;
        if let Some(tb) = self.trust_boundaries.get(idx) {
            for &c in &tb.components {
                bump(&mut self.refs.component_boundaries, Some(c.index()), true);
            }
        }
        Ok(Op::RemoveTrustBoundary { id })
    }

    fn commit_rename_trust_boundary(
        &mut self,
        id: TrustBoundaryId,
        name: Arc<str>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let tb = self.trust_boundary_mut(id)?;
        let name = std::mem::replace(&mut tb.name, name);
        let provenance = std::mem::replace(&mut tb.provenance, provenance);
        Ok(Op::RenameTrustBoundary {
            id,
            name,
            provenance,
        })
    }

    fn commit_boundary_component(
        &mut self,
        boundary: TrustBoundaryId,
        component: ComponentId,
        provenance: Provenance,
        link: bool,
    ) -> Result<Op, ModelError> {
        let idx = self.trust_boundary_index(boundary)?;
        let c = self.component_index(component)?;
        let tb = self
            .trust_boundaries
            .get(idx)
            .ok_or(ModelError::UnknownTrustBoundary(boundary))?;
        let found = self.find_boundary_component(tb, component);
        let tb = self
            .trust_boundaries
            .get_mut(idx)
            .ok_or(ModelError::UnknownTrustBoundary(boundary))?;
        match (link, found) {
            (true, Err(pos)) => {
                tb.components.insert(pos, c);
                let provenance = std::mem::replace(&mut tb.provenance, provenance);
                bump(&mut self.refs.component_boundaries, Some(c.index()), true);
                Ok(Op::RemoveBoundaryComponent {
                    boundary,
                    component,
                    provenance,
                })
            }
            (false, Ok(pos)) => {
                tb.components.remove(pos);
                let provenance = std::mem::replace(&mut tb.provenance, provenance);
                bump(&mut self.refs.component_boundaries, Some(c.index()), false);
                Ok(Op::AddBoundaryComponent {
                    boundary,
                    component,
                    provenance,
                })
            }
            (true, Ok(_)) => Err(ModelError::DuplicateBoundaryComponent {
                boundary,
                component,
            }),
            (false, Err(_)) => Err(ModelError::BoundaryComponentNotLinked {
                boundary,
                component,
            }),
        }
    }

    fn commit_remove_trust_boundary(&mut self, id: TrustBoundaryId) -> Result<Op, ModelError> {
        let idx = self.trust_boundary_index(id)?;
        let (_, tb) = self.live_trust_boundary(id)?;
        let components = tb
            .components
            .iter()
            .map(|&c| {
                self.component_key(c)
                    .ok_or(ModelError::UnknownTrustBoundary(id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tb = self
            .trust_boundaries
            .remove(idx)
            .ok_or(ModelError::UnknownTrustBoundary(id))?;
        for &c in &tb.components {
            bump(&mut self.refs.component_boundaries, Some(c.index()), false);
        }
        Ok(Op::AddTrustBoundary {
            id,
            name: tb.name,
            components,
            provenance: tb.provenance,
        })
    }

    fn commit_set_asset_component(
        &mut self,
        asset: AssetId,
        component: Option<ComponentId>,
        provenance: Provenance,
    ) -> Result<Op, ModelError> {
        let idx = self.asset_index(asset)?;
        let new_idx = component.map(|c| self.component_index(c)).transpose()?;
        let old_idx = self
            .assets
            .get(idx)
            .ok_or(ModelError::UnknownAsset(asset))?
            .component;
        let old_component = old_idx
            .map(|i| self.components.id(i).ok_or(ModelError::UnknownAsset(asset)))
            .transpose()?;
        let a = self
            .assets
            .get_mut(idx)
            .ok_or(ModelError::UnknownAsset(asset))?;
        a.component = new_idx;
        let provenance = std::mem::replace(&mut a.provenance, provenance);
        if let Some(old) = old_idx {
            bump(&mut self.refs.component_assets, Some(old.index()), false);
        }
        if let Some(new) = new_idx {
            bump(&mut self.refs.component_assets, Some(new.index()), true);
        }
        Ok(Op::SetAssetComponent {
            asset,
            component: old_component,
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
            && self.descriptions == other.descriptions
            && self.review_marks == other.review_marks
            && self.languages() == other.languages()
            && self.elements.same_live(&other.elements)
            && self.assets.len() == other.assets.len()
            && self.damages.len() == other.damages.len()
            && self.threats.len() == other.threats.len()
            && self.trees.len() == other.trees.len()
            && self.controls.len() == other.controls.len()
            && self.goals.len() == other.goals.len()
            && self.claims.len() == other.claims.len()
            && self.parties.len() == other.parties.len()
            && self.components.len() == other.components.len()
            && self.interfaces.len() == other.interfaces.len()
            && self.data_flows.len() == other.data_flows.len()
            && self.trust_boundaries.len() == other.trust_boundaries.len()
            && self
                .assets()
                .zip(other.assets())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_asset(x, other, y))
            && self
                .damage_scenarios()
                .zip(other.damage_scenarios())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .security_controls()
                .zip(other.security_controls())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_control(x, other, y))
            && self
                .cybersecurity_claims()
                .zip(other.cybersecurity_claims())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_claim(x, other, y))
            && self
                .cybersecurity_goals()
                .zip(other.cybersecurity_goals())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_goal(x, other, y))
            && self
                .parties()
                .zip(other.parties())
                .all(|((_, a, x), (_, b, y))| a == b && x == y)
            && self
                .components()
                .zip(other.components())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_component(x, other, y))
            && self
                .interfaces()
                .zip(other.interfaces())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_interface(x, other, y))
            && self
                .data_flows()
                .zip(other.data_flows())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_data_flow(x, other, y))
            && self
                .trust_boundaries()
                .zip(other.trust_boundaries())
                .all(|((_, a, x), (_, b, y))| a == b && self.same_trust_boundary(x, other, y))
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
            && x.step_factors == y.step_factors
            && (0..x.tree.len()).all(|i| {
                x.attack_type_at(i) == y.attack_type_at(i)
                    && self.item_ref(x.via_at(i)) == other.item_ref(y.via_at(i))
            })
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

    /// Compares two assets from two projects by content, with optional
    /// component bindings resolved to public IDs. O(1).
    fn same_asset(&self, x: &Asset, other: &Project, y: &Asset) -> bool {
        x.name == y.name
            && x.properties == y.properties
            && x.provenance == y.provenance
            && self.component_opt_key(x.component) == other.component_opt_key(y.component)
    }

    /// Compares two components from two projects by content, with optional
    /// parent references resolved to public IDs. O(1).
    fn same_component(&self, x: &Component, other: &Project, y: &Component) -> bool {
        x.name == y.name
            && x.system == y.system
            && x.provenance == y.provenance
            && self.component_opt_key(x.parent) == other.component_opt_key(y.parent)
    }

    /// Compares two interfaces from two projects by content, with component
    /// references resolved to public IDs. O(1).
    fn same_interface(&self, x: &Interface, other: &Project, y: &Interface) -> bool {
        x.name == y.name
            && x.kind == y.kind
            && x.provenance == y.provenance
            && self.component_key(x.component) == other.component_key(y.component)
    }

    /// Compares two data flows from two projects by content, with component
    /// and interface references resolved to public IDs. O(1).
    fn same_data_flow(&self, x: &DataFlow, other: &Project, y: &DataFlow) -> bool {
        x.name == y.name
            && x.provenance == y.provenance
            && self.component_key(x.from) == other.component_key(y.from)
            && self.component_key(x.to) == other.component_key(y.to)
            && self.interface_opt_key(x.interface) == other.interface_opt_key(y.interface)
    }

    /// Compares two trust boundaries from two projects by content, with member
    /// component references resolved to public IDs. O(m).
    fn same_trust_boundary(&self, x: &TrustBoundary, other: &Project, y: &TrustBoundary) -> bool {
        x.name == y.name
            && x.provenance == y.provenance
            && x.components.len() == y.components.len()
            && x.components
                .iter()
                .zip(&y.components)
                .all(|(&a, &b)| self.component_key(a) == other.component_key(b))
    }

    /// Compares two controls from two projects by content, with applied target
    /// items resolved to public IDs. O(t + r).
    fn same_control(&self, x: &SecurityControl, other: &Project, y: &SecurityControl) -> bool {
        x.name == y.name
            && x.rules == y.rules
            && x.kind == y.kind
            && x.provenance == y.provenance
            && x.targets.len() == y.targets.len()
            && x.targets
                .iter()
                .zip(&y.targets)
                .all(|(&a, &b)| self.item_ref(Some(a)) == other.item_ref(Some(b)))
    }

    /// Compares two goals from two projects by content, with control and
    /// responsible-party references resolved to public IDs. O(c + p).
    fn same_goal(&self, x: &CybersecurityGoal, other: &Project, y: &CybersecurityGoal) -> bool {
        x.name == y.name
            && x.provenance == y.provenance
            && x.controls.len() == y.controls.len()
            && x.controls
                .iter()
                .zip(&y.controls)
                .all(|(&a, &b)| self.control_key(a) == other.control_key(b))
            && x.responsible.len() == y.responsible.len()
            && x.responsible
                .iter()
                .zip(&y.responsible)
                .all(|(&a, &b)| self.party_key(a) == other.party_key(b))
    }

    /// Compares two claims from two projects by content, with responsible-party
    /// references resolved to public IDs. O(p).
    fn same_claim(&self, x: &CybersecurityClaim, other: &Project, y: &CybersecurityClaim) -> bool {
        x.name == y.name
            && x.provenance == y.provenance
            && x.responsible.len() == y.responsible.len()
            && x.responsible
                .iter()
                .zip(&y.responsible)
                .all(|(&a, &b)| self.party_key(a) == other.party_key(b))
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
            && self.goal_key(x.goal) == other.goal_key(y.goal)
            && self.claim_key(x.claim) == other.claim_key(y.claim)
            && x.damages.len() == y.damages.len()
            && x.damages
                .iter()
                .zip(&y.damages)
                .all(|(&a, &b)| self.damage_key(a) == other.damage_key(b))
            && x.damage_rationales.len() == y.damage_rationales.len()
            && x.damage_rationales
                .iter()
                .zip(&y.damage_rationales)
                .all(|((a, ta), (b, tb))| self.damage_key(*a) == other.damage_key(*b) && ta == tb)
    }

    /// Checks every structural invariant and returns a description of the
    /// first one broken. Time O(n log n + L + m).
    #[cfg(test)]
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        arena_invariants("asset", &self.assets)?;
        arena_invariants("damage", &self.damages)?;
        arena_invariants("threat", &self.threats)?;
        arena_invariants("tree", &self.trees)?;
        arena_invariants("control", &self.controls)?;
        arena_invariants("goal", &self.goals)?;
        arena_invariants("claim", &self.claims)?;
        arena_invariants("component", &self.components)?;
        arena_invariants("interface", &self.interfaces)?;
        arena_invariants("data_flow", &self.data_flows)?;
        arena_invariants("trust_boundary", &self.trust_boundaries)?;
        let (control_goals, component_assets, component_controls, interface_controls) =
            self.entity_invariants()?;
        let (
            component_children,
            component_interfaces,
            component_flows,
            component_boundaries,
            interfaces,
        ) = self.item_invariants()?;
        let controls = self.tree_invariants()?;
        let (asset_properties, assets, damages, trees, goals, claims) = self.threat_invariants()?;
        let want = RefCounts {
            asset_properties,
            assets,
            damages,
            trees,
            controls,
            control_goals,
            goals,
            claims,
            component_children,
            component_interfaces,
            component_flows,
            component_boundaries,
            component_assets,
            component_controls,
            interfaces,
            interface_controls,
        };
        self.refs_invariants(&want)
    }

    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    fn threat_invariants(
        &self,
    ) -> Result<
        (
            Vec<Box<[PropertyCount]>>,
            Vec<u32>,
            Vec<u32>,
            Vec<u32>,
            Vec<u32>,
            Vec<u32>,
        ),
        String,
    > {
        let m = &*self.methodology;
        let mut assets = vec![0u32; self.assets.slot_count()];
        let mut damages = vec![0u32; self.damages.slot_count()];
        let mut trees = vec![0u32; self.trees.slot_count()];
        let mut goals = vec![0u32; self.goals.slot_count()];
        let mut claims = vec![0u32; self.claims.slot_count()];
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
            if t.catalog_refs.len() > MAX_CATALOG_REFS || t.damages.len() > MAX_DAMAGE_LINKS {
                return Err(format!("threat {id} exceeds link cap"));
            }
            if !t.catalog_refs.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!("threat {id} catalog refs not sorted and unique"));
            }
            for r in &t.catalog_refs {
                m.check_catalog_ref(r)
                    .map_err(|e| format!("threat {id}: {e}"))?;
            }
            let keys: Vec<_> = t.damages.iter().map(|&d| self.damage_key(d)).collect();
            if keys.iter().any(Option::is_none) || !keys.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!("threat {id} damages invalid or not sorted"));
            }
            if t.damage_rationales
                .iter()
                .any(|(d, text)| text.is_empty() || !t.damages.contains(d))
            {
                return Err(format!("threat {id} damage rationale unlinked or empty"));
            }
            let rkeys: Vec<_> = t
                .damage_rationales
                .iter()
                .map(|(d, _)| self.damage_key(*d))
                .collect();
            if rkeys.iter().any(Option::is_none) || !rkeys.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!(
                    "threat {id} damage rationales invalid or not sorted"
                ));
            }
            if let Some(level) = t.direct_feasibility {
                m.check_feasibility_level(level)
                    .map_err(|e| format!("threat {id}: {e}"))?;
            }
            if let Some(tree) = t.attack_tree {
                self.trees
                    .get(tree)
                    .ok_or_else(|| format!("threat {id} links a dead tree slot"))?;
                bump(&mut trees, Some(tree.index()), true);
            }
            if let Some(goal) = t.goal {
                self.goals
                    .get(goal)
                    .ok_or_else(|| format!("threat {id} links a dead goal slot"))?;
                bump(&mut goals, Some(goal.index()), true);
            }
            if let Some(claim) = t.claim {
                self.claims
                    .get(claim)
                    .ok_or_else(|| format!("threat {id} links a dead claim slot"))?;
                bump(&mut claims, Some(claim.index()), true);
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
        Ok((
            rows.into_iter().map(Vec::into_boxed_slice).collect(),
            assets,
            damages,
            trees,
            goals,
            claims,
        ))
    }

    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    fn entity_invariants(&self) -> Result<(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>), String> {
        let m = &*self.methodology;
        let mut component_assets = vec![0u32; self.components.slot_count()];
        let mut component_controls = vec![0u32; self.components.slot_count()];
        let mut interface_controls = vec![0u32; self.interfaces.slot_count()];

        for (_, id, a) in self.assets() {
            check_name(EntityKind::Asset, &a.name).map_err(|e| format!("asset {id}: {e}"))?;
            check_provenance(&a.provenance).map_err(|e| format!("asset {id}: {e}"))?;
            if a.properties.is_empty() {
                return Err(format!("asset {id} has no properties"));
            }
            m.check_properties(a.properties)
                .map_err(|e| format!("asset {id}: {e}"))?;
            if let Some(comp) = a.component {
                if self.components.get(comp).is_none() {
                    return Err(format!("asset {id} binds to a dead component slot"));
                }
                bump(&mut component_assets, Some(comp.index()), true);
            }
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
            self.check_security_control_invariants(
                id,
                c,
                &mut component_controls,
                &mut interface_controls,
            )?;
        }
        let mut control_goals = vec![0u32; self.controls.slot_count()];
        for (_, id, g) in self.cybersecurity_goals() {
            check_name(EntityKind::CybersecurityGoal, &g.name)
                .map_err(|e| format!("goal {id}: {e}"))?;
            check_provenance(&g.provenance).map_err(|e| format!("goal {id}: {e}"))?;
            if g.controls.len() > MAX_GOAL_CONTROLS {
                return Err(format!("goal {id} has too many controls"));
            }
            let keys: Vec<_> = g.controls.iter().map(|&c| self.control_key(c)).collect();
            if keys.iter().any(Option::is_none) {
                return Err(format!("goal {id} links a dead control slot"));
            }
            if !keys.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!("goal {id} controls not sorted and unique"));
            }
            for &c in &g.controls {
                bump(&mut control_goals, Some(c.index()), true);
            }
        }
        for (_, id, c) in self.cybersecurity_claims() {
            check_name(EntityKind::CybersecurityClaim, &c.name)
                .map_err(|e| format!("claim {id}: {e}"))?;
            check_provenance(&c.provenance).map_err(|e| format!("claim {id}: {e}"))?;
        }
        Ok((
            control_goals,
            component_assets,
            component_controls,
            interface_controls,
        ))
    }

    #[cfg(test)]
    fn check_security_control_invariants(
        &self,
        id: ControlId,
        c: &SecurityControl,
        component_controls: &mut [u32],
        interface_controls: &mut [u32],
    ) -> Result<(), String> {
        let m = &*self.methodology;
        check_name(EntityKind::SecurityControl, &c.name)
            .map_err(|e| format!("control {id}: {e}"))?;
        check_provenance(&c.provenance).map_err(|e| format!("control {id}: {e}"))?;
        if c.targets.len() > MAX_CONTROL_TARGETS {
            return Err(format!("control {id} has too many targets"));
        }
        let target_refs: Vec<_> = c.targets.iter().map(|&t| self.item_ref(Some(t))).collect();
        if target_refs.iter().any(Option::is_none) {
            return Err(format!("control {id} targets a dead item slot"));
        }
        if !target_refs.windows(2).all(|w| w[0] < w[1]) {
            return Err(format!("control {id} targets not sorted and unique"));
        }
        for &t in &c.targets {
            match t {
                ItemIdx::Component(comp) => bump(component_controls, Some(comp.index()), true),
                ItemIdx::Interface(iface) => bump(interface_controls, Some(iface.index()), true),
            }
        }
        if c.rules.len() > MAX_CONTROL_RULES {
            return Err(format!("control {id} has too many rules"));
        }
        if !c
            .rules
            .windows(2)
            .all(|w| w[0].attack_type < w[1].attack_type)
        {
            return Err(format!("control {id} rules not sorted and unique"));
        }
        for r in &c.rules {
            m.check_attack_type(r.attack_type)
                .map_err(|e| format!("control {id}: {e}"))?;
            m.check_feasibility_level(r.ceiling)
                .map_err(|e| format!("control {id}: {e}"))?;
        }
        Ok(())
    }

    #[cfg(test)]
    #[allow(clippy::type_complexity)]
    fn item_invariants(
        &self,
    ) -> Result<(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>), String> {
        let m = &*self.methodology;
        let mut component_children = vec![0u32; self.components.slot_count()];
        let mut component_interfaces = vec![0u32; self.components.slot_count()];
        let mut component_flows = vec![0u32; self.components.slot_count()];
        let mut component_boundaries = vec![0u32; self.components.slot_count()];
        let mut interfaces = vec![0u32; self.interfaces.slot_count()];

        for (idx, id, c) in self.components() {
            check_name(EntityKind::Component, &c.name)
                .map_err(|e| format!("component {id}: {e}"))?;
            check_provenance(&c.provenance).map_err(|e| format!("component {id}: {e}"))?;
            if let Some(p_idx) = c.parent {
                let p_id = self
                    .components
                    .id(p_idx)
                    .ok_or_else(|| format!("component {id} has a dead parent slot"))?;
                self.check_component_parent_chain(id, Some(idx), Some(p_id))
                    .map_err(|e| format!("component {id}: {e}"))?;
                bump(&mut component_children, Some(p_idx.index()), true);
            }
        }
        for (_, id, iface) in self.interfaces() {
            check_name(EntityKind::Interface, &iface.name)
                .map_err(|e| format!("interface {id}: {e}"))?;
            check_provenance(&iface.provenance).map_err(|e| format!("interface {id}: {e}"))?;
            m.check_interface_kind(iface.kind)
                .map_err(|e| format!("interface {id}: {e}"))?;
            if self.components.get(iface.component).is_none() {
                return Err(format!("interface {id} belongs to a dead component slot"));
            }
            bump(
                &mut component_interfaces,
                Some(iface.component.index()),
                true,
            );
        }
        for (_, id, df) in self.data_flows() {
            check_name(EntityKind::DataFlow, &df.name)
                .map_err(|e| format!("data flow {id}: {e}"))?;
            check_provenance(&df.provenance).map_err(|e| format!("data flow {id}: {e}"))?;
            if self.components.get(df.from).is_none() || self.components.get(df.to).is_none() {
                return Err(format!("data flow {id} endpoints a dead component slot"));
            }
            bump(&mut component_flows, Some(df.from.index()), true);
            if df.to != df.from {
                bump(&mut component_flows, Some(df.to.index()), true);
            }
            if let Some(i_idx) = df.interface {
                let Some(iface) = self.interfaces.get(i_idx) else {
                    return Err(format!("data flow {id} references a dead interface slot"));
                };
                if iface.component != df.from && iface.component != df.to {
                    return Err(format!(
                        "data flow {id} interface does not belong to either endpoint"
                    ));
                }
                bump(&mut interfaces, Some(i_idx.index()), true);
            }
        }
        for (_, id, tb) in self.trust_boundaries() {
            check_name(EntityKind::TrustBoundary, &tb.name)
                .map_err(|e| format!("trust boundary {id}: {e}"))?;
            check_provenance(&tb.provenance).map_err(|e| format!("trust boundary {id}: {e}"))?;
            if tb.components.len() > MAX_TRUST_BOUNDARY_MEMBERS {
                return Err(format!("trust boundary {id} has too many members"));
            }
            let keys: Vec<_> = tb
                .components
                .iter()
                .map(|&c| self.component_key(c))
                .collect();
            if keys.iter().any(Option::is_none) {
                return Err(format!("trust boundary {id} links a dead component slot"));
            }
            if !keys.windows(2).all(|w| w[0] < w[1]) {
                return Err(format!(
                    "trust boundary {id} components not sorted and unique"
                ));
            }
            for &c in &tb.components {
                bump(&mut component_boundaries, Some(c.index()), true);
            }
        }
        Ok((
            component_children,
            component_interfaces,
            component_flows,
            component_boundaries,
            interfaces,
        ))
    }

    /// The "through" items and attack types of one tree, for
    /// `tree_invariants` (tests only).
    #[cfg(test)]
    fn check_step_links(
        &self,
        id: TreeId,
        entry: &TreeEntry,
        m: &Methodology,
    ) -> Result<(), String> {
        if let Some(links) = &entry.step_links
            && (links.via.len() != entry.tree.len()
                || links.attack_type.len() != entry.tree.len()
                || links.is_empty())
        {
            return Err(format!("tree {id} has malformed via slots"));
        }
        for (i, (_, node_id, node)) in entry.tree.iter().enumerate() {
            let (via, attack_type) = (entry.via_at(i), entry.attack_type_at(i));
            if matches!(node, Node::Gate { .. }) && (via.is_some() || attack_type.is_some()) {
                return Err(format!("tree {id} gate {node_id} has a via or attack type"));
            }
            if via.is_some() && self.item_ref(via).is_none() {
                return Err(format!("tree {id} node {node_id} goes via a dead item"));
            }
            if let Some(t) = attack_type {
                m.check_attack_type(t)
                    .map_err(|e| format!("tree {id} node {node_id}: {e}"))?;
            }
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
            if entry.step_factors.len() != entry.tree.len() {
                return Err(format!(
                    "tree {id} has {} factor slots",
                    entry.step_factors.len()
                ));
            }
            if entry.step_mitigations.len() != entry.tree.len() {
                return Err(format!(
                    "tree {id} has {} mitigation slots",
                    entry.step_mitigations.len()
                ));
            }
            self.check_step_links(id, entry, m)?;
            entry
                .tree
                .topological_order()
                .map_err(|e| format!("tree {id}: {e}"))?;
            for ((((_, node_id, node), &level), factors), mitigations) in entry
                .tree
                .iter()
                .zip(&entry.step_feasibility)
                .zip(&entry.step_factors)
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
                if matches!(node, Node::Gate { .. }) && !factors.is_empty() {
                    return Err(format!("tree {id} gate {node_id} has step factors"));
                }
                if !factors.is_empty() {
                    if factors.len() != m.feasibility_factor_count() {
                        return Err(format!(
                            "tree {id} node {node_id} has {} factor slots",
                            factors.len()
                        ));
                    }
                    if factors.iter().all(Option::is_none) {
                        return Err(format!(
                            "tree {id} node {node_id} stores an all-None factor row"
                        ));
                    }
                    for (f, &fl) in m.feasibility_factors().zip(factors) {
                        if let Some(fl) = fl {
                            m.check_factor_level(f, fl)
                                .map_err(|e| format!("tree {id} node {node_id}: {e}"))?;
                        }
                    }
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
        if have.control_goals != want.control_goals {
            return Err(format!(
                "control goal counts differ from a recount: {:?} vs {:?}",
                have.control_goals, want.control_goals
            ));
        }
        if have.goals != want.goals {
            return Err(format!(
                "goal counts differ from a recount: {:?} vs {:?}",
                have.goals, want.goals
            ));
        }
        if have.claims != want.claims {
            return Err(format!(
                "claim counts differ from a recount: {:?} vs {:?}",
                have.claims, want.claims
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

// ---------------------------------------------------------------------------
// Incremental evaluation cache (rules A1, D10).
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedTreeEval {
    inherent: Result<TreeEvalCache<FeasibilityLevel>, ModelError>,
    residual: Result<TreeEvalCache<FeasibilityLevel>, ModelError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedThreatEval {
    inherent: Result<ThreatRisk, ModelError>,
    residual: Result<ThreatRisk, ModelError>,
}

/// Dependency-tracked incremental evaluation cache over a [`Project`]
/// (rules A1, A2, D10).
///
/// Keeps derived attack-tree evaluations ([`Evaluation<FeasibilityLevel>`])
/// and threat risk determinations ([`ThreatRisk`]) separate from the source
/// [`Project`] state. The cache can be dropped and rebuilt at any time via
/// [`ProjectEvalCache::new`] or [`rebuild`](Self::rebuild), or maintained
/// incrementally across [`Op`] edits via [`Project::apply_cached`] or
/// [`update`](Self::update).
///
/// Dependency tracking operates at two levels:
/// 1. **Intra-tree DAG level**: each attack tree caches an inherent and a
///    residual [`TreeEvalCache`]. A single step rating or mitigation change
///    re-evaluates only the changed step and its affected ancestor gates in
///    reverse topological rank order (`O(depth x fan-out)`).
/// 2. **Cross-entity project level**: reverse dependency lists map each
///    `TreeIdx` and `DamageIdx` to the `ThreatIdx`s that reference them.
///    When a step rating changes, only the threats linked to that tree sync
///    the touched node slots in place (`O(depth)` without cloning the rest
///    of the tree evaluation) and recompute their risk matrix entries only
///    if the tree's root feasibility changed. When a damage impact changes
///    ([`Op::SetImpact`]), no attack tree is re-evaluated and only the
///    threats linked to that damage scenario update their risk entries in
///    place. Metadata, asset, control, goal, and claim edits do zero
///    evaluation work (`O(1)`).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AssetId, AttackTree, ControlId, DamageId, GateKind, Methodology, Node, NodeId, Op,
///     Project, ProjectEvalCache, Provenance, ThreatId, TreeId,
/// };
///
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
/// let (low, med, high) = (
///     m.feasibility_level("low").ok_or("no level")?,
///     m.feasibility_level("medium").ok_or("no level")?,
///     m.feasibility_level("high").ok_or("no level")?,
/// );
/// let (root, s1, s2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
/// let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise"))?;
/// tree.add_child(tree.root(), s1, Node::step("Path A"))?;
/// tree.add_child(tree.root(), s2, Node::step("Path B"))?;
/// let tree_id = TreeId::new_v7();
/// project.apply(Op::AddAttackTree {
///     id: tree_id,
///     tree,
///     step_feasibility: vec![(s1, high), (s2, med)],
///     provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetThreatAttackTree {
///     threat, tree: Some(tree_id), provenance: Provenance::default(),
/// })?;
///
/// let mut cache = ProjectEvalCache::new(&project);
/// assert_eq!(cache.evaluate_risk(&project, threat)?.max_risk(), m.risk_level("r5"));
///
/// // Mitigating s1 to low affects only residual evaluation (inherent stays r5).
/// let ctrl = ControlId::new_v7();
/// project.apply_cached(
///     Op::AddSecurityControl { id: ctrl, name: "SecOC".into(), provenance: Provenance::default() },
///     &mut cache,
/// )?;
/// assert_eq!(cache.recomputed_nodes(), 0);
///
/// project.apply_cached(
///     Op::SetStepMitigation {
///         tree: tree_id, node: s1, control: ctrl, feasibility: Some(low),
///         provenance: Provenance::default(),
///     },
///     &mut cache,
/// )?;
/// assert_eq!(cache.evaluate_risk(&project, threat)?.max_risk(), m.risk_level("r5"));
/// assert_eq!(cache.evaluate_residual_risk(&project, threat)?.max_risk(), m.risk_level("r4"));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct ProjectEvalCache {
    trees: Vec<Option<CachedTreeEval>>,
    threats: Vec<Option<CachedThreatEval>>,
    tree_threats: Vec<Vec<ThreatIdx>>,
    damage_threats: Vec<Vec<ThreatIdx>>,
    recomputed_nodes: u64,
    recomputed_threats: u64,
}

impl PartialEq for ProjectEvalCache {
    fn eq(&self, other: &Self) -> bool {
        self.trees == other.trees
            && self.threats == other.threats
            && self.tree_threats == other.tree_threats
            && self.damage_threats == other.damage_threats
    }
}

impl Eq for ProjectEvalCache {}

impl ProjectEvalCache {
    /// Builds a complete evaluation cache over `project`.
    ///
    /// Time O(K * (V + E) + T * (V + d * c)), space O(K * V + T * (V + d * c)).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ProjectEvalCache};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(m);
    /// let cache = ProjectEvalCache::new(&project);
    /// assert_eq!(cache.recomputed_nodes(), 0);
    /// assert_eq!(cache.recomputed_threats(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new(project: &Project) -> Self {
        let mut cache = Self {
            trees: Vec::new(),
            threats: Vec::new(),
            tree_threats: Vec::new(),
            damage_threats: Vec::new(),
            recomputed_nodes: 0,
            recomputed_threats: 0,
        };
        cache.rebuild(project);
        cache
    }

    /// Drops all cached evaluations and rebuilds them from `project` in
    /// place (rule D10), resetting the diagnostic recomputation counters to
    /// zero.
    ///
    /// Time O(K * (V + E) + T * (V + d * c)), space O(K * V + T * (V + d * c)).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ProjectEvalCache};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(m);
    /// let mut cache = ProjectEvalCache::new(&project);
    /// cache.rebuild(&project);
    /// assert_eq!(cache.recomputed_nodes(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn rebuild(&mut self, project: &Project) {
        self.trees.clear();
        self.trees.resize_with(project.trees.slot_count(), || None);
        self.tree_threats.clear();
        self.tree_threats
            .resize_with(project.trees.slot_count(), Vec::new);
        self.damage_threats.clear();
        self.damage_threats
            .resize_with(project.damages.slot_count(), Vec::new);
        self.threats.clear();
        self.threats
            .resize_with(project.threats.slot_count(), || None);
        self.recomputed_nodes = 0;
        self.recomputed_threats = 0;

        for (idx, maybe_entry) in project.trees.slots.iter().enumerate() {
            if let Some(entry) = maybe_entry {
                self.trees[idx] = Some(Self::build_tree_eval(project, entry));
            }
        }

        for (pos, maybe_threat) in project.threats.slots.iter().enumerate() {
            let (Some(t), Some(&threat_id), Some(threat_idx)) = (
                maybe_threat.as_ref(),
                project.threats.ids.get(pos),
                u32::try_from(pos).ok().map(ThreatIdx::from_u32),
            ) else {
                continue;
            };
            if let Some(tree_idx) = t.attack_tree
                && let Some(list) = self.tree_threats.get_mut(tree_idx.index())
            {
                insert_sorted_unique(list, threat_idx);
            }
            for &damage_idx in &t.damages {
                if let Some(list) = self.damage_threats.get_mut(damage_idx.index()) {
                    insert_sorted_unique(list, threat_idx);
                }
            }
            self.threats[pos] = Some(self.build_threat_eval(project, threat_id, t));
        }
    }

    /// Incrementally updates the cache after `op` has been applied to
    /// `project` (rule A1).
    ///
    /// Accepts either the forward [`Op`] that was passed to
    /// [`Project::apply`] or the inverse [`Op`] returned by it.
    ///
    /// Time O(depth x fan-out) for a single step rating or mitigation
    /// change, O(linked threats x d x c) for a damage impact change, and
    /// O(1) for metadata, asset, control, goal, and claim operations.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project,
    /// #     ProjectEvalCache, Provenance, ThreatId, TreeId,
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
    /// let (low, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
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
    ///
    /// let mut cache = ProjectEvalCache::new(&project);
    /// let inv = project.apply(Op::SetStepFeasibility {
    ///     tree: tree_id, node: step, level: Some(low), provenance: Provenance::default(),
    /// })?;
    /// cache.update(&project, &inv);
    /// assert_eq!(cache.evaluate_risk(&project, threat)?, &project.evaluate_risk(threat)?);
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_lines)] // one arm per op kind
    pub fn update(&mut self, project: &Project, op: &Op) {
        if !self.arenas_compatible(project, op) {
            self.rebuild(project);
            return;
        }
        match op {
            Op::SetDescription { .. }
            | Op::SetDescriptionTranslation { .. }
            | Op::SetProjectLanguages { .. }
            | Op::SetReviewMark { .. }
            | Op::RenameTreeNode { .. }
            | Op::AddAsset { .. }
            | Op::RenameAsset { .. }
            | Op::SetAssetProperties { .. }
            | Op::RemoveAsset { .. }
            | Op::RenameDamageScenario { .. }
            | Op::RenameThreatScenario { .. }
            | Op::SetThreatCategory { .. }
            | Op::AddCatalogRef { .. }
            | Op::RemoveCatalogRef { .. }
            | Op::SetRiskTreatment { .. }
            | Op::SetThreatGoal { .. }
            | Op::SetThreatClaim { .. }
            | Op::SetThreatScaling { .. }
            | Op::SetImpactRationale { .. }
            | Op::SetFeasibilityRationale { .. }
            | Op::SetDamageLinkRationale { .. }
            | Op::AddSecurityControl { .. }
            | Op::RenameSecurityControl { .. }
            | Op::SetControlKind { .. }
            | Op::RemoveSecurityControl { .. }
            | Op::AddCybersecurityGoal { .. }
            | Op::RenameCybersecurityGoal { .. }
            | Op::LinkGoalControl { .. }
            | Op::UnlinkGoalControl { .. }
            | Op::LinkGoalParty { .. }
            | Op::UnlinkGoalParty { .. }
            | Op::RemoveCybersecurityGoal { .. }
            | Op::AddCybersecurityClaim { .. }
            | Op::RenameCybersecurityClaim { .. }
            | Op::LinkClaimParty { .. }
            | Op::UnlinkClaimParty { .. }
            | Op::RemoveCybersecurityClaim { .. }
            | Op::AddParty { .. }
            | Op::RenameParty { .. }
            | Op::SetPartyKind { .. }
            | Op::RemoveParty { .. }
            | Op::AddElement { .. }
            | Op::RenameElement { .. }
            | Op::RemoveElement { .. }
            | Op::SetElementAttribute { .. }
            | Op::LinkElement { .. }
            | Op::UnlinkElement { .. }
            | Op::AddComponent { .. }
            | Op::RenameComponent { .. }
            | Op::SetComponentParent { .. }
            | Op::SetComponentSystem { .. }
            | Op::RemoveComponent { .. }
            | Op::AddInterface { .. }
            | Op::RenameInterface { .. }
            | Op::SetInterfaceKind { .. }
            | Op::RemoveInterface { .. }
            | Op::AddDataFlow { .. }
            | Op::RenameDataFlow { .. }
            | Op::SetDataFlowInterface { .. }
            | Op::RemoveDataFlow { .. }
            | Op::AddTrustBoundary { .. }
            | Op::RenameTrustBoundary { .. }
            | Op::AddBoundaryComponent { .. }
            | Op::RemoveBoundaryComponent { .. }
            | Op::RemoveTrustBoundary { .. }
            | Op::SetAssetComponent { .. } => {}

            Op::AddDamageScenario { .. } | Op::RemoveDamageScenario { .. } => {
                self.damage_threats
                    .resize_with(project.damages.slot_count(), Vec::new);
            }

            Op::SetImpact { id, .. } => self.update_damage_impact(project, *id),
            Op::AddAttackTree { id, .. } | Op::RemoveAttackTree { id } => {
                if let Some(tree_idx) = project.attack_tree_idx(id) {
                    self.update_tree_added(project, tree_idx);
                } else {
                    self.update_tree_removed(project);
                }
            }
            Op::AddTreeNode { tree, .. }
            | Op::RemoveTreeNode { tree, .. }
            | Op::LinkTreeNode { tree, .. }
            | Op::UnlinkTreeNode { tree, .. }
            | Op::SetTreeNodeGate { tree, .. } => self.update_tree_structure(project, *tree),
            Op::SetStepFeasibility { tree, node, .. } | Op::SetStepFactor { tree, node, .. } => {
                self.update_step(project, *tree, *node, true);
            }
            Op::SetStepMitigation { tree, node, .. }
            | Op::SetStepVia { tree, node, .. }
            | Op::SetStepAttackType { tree, node, .. } => {
                self.update_step(project, *tree, *node, false);
            }
            Op::ApplyControl { target, .. } | Op::UnapplyControl { target, .. } => {
                self.update_control_target(project, *target);
            }
            Op::SetControlRule {
                control,
                attack_type,
                ..
            } => self.update_control_rule(project, *control, *attack_type),
            Op::AddThreatScenario { id, .. } | Op::RemoveThreatScenario { id } => {
                if let Some(threat_idx) = project.threat_scenario_idx(id) {
                    self.update_threat_added(project, threat_idx, *id);
                } else {
                    self.update_threat_removed(project);
                }
            }
            Op::SetThreatFeasibility { threat, .. } => {
                self.update_threat_feasibility(project, *threat);
            }
            Op::SetThreatAttackTree { threat, .. } => self.update_threat_tree(project, *threat),
            Op::LinkDamage { threat, .. } | Op::UnlinkDamage { threat, .. } => {
                self.update_threat_damages(project, *threat);
            }
        }
    }

    /// Returns the cached inherent [`Evaluation`] of attack tree `id`.
    ///
    /// Time O(log K) (public ID lookup + slot borrow), space O(1).
    ///
    /// # Errors
    ///
    /// Same as [`Project::evaluate_tree`].
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_tree(&project, tree_id)?.root_value(), &high);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_tree(
        &self,
        project: &Project,
        id: TreeId,
    ) -> Result<&Evaluation<FeasibilityLevel>, ModelError> {
        let (idx, _) = project.live_tree(id)?;
        let slot = self
            .trees
            .get(idx.index())
            .and_then(Option::as_ref)
            .ok_or(ModelError::UnknownAttackTree(id))?;
        slot.inherent
            .as_ref()
            .map(TreeEvalCache::evaluation)
            .map_err(Clone::clone)
    }

    /// Returns the cached residual [`Evaluation`] of attack tree `id`.
    ///
    /// Time O(log K) (public ID lookup + slot borrow), space O(1).
    ///
    /// # Errors
    ///
    /// Same as [`Project::evaluate_residual_tree`].
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let (low, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "SecOC".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_residual_tree(&project, tree_id)?.root_value(), &low);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_residual_tree(
        &self,
        project: &Project,
        id: TreeId,
    ) -> Result<&Evaluation<FeasibilityLevel>, ModelError> {
        let (idx, _) = project.live_tree(id)?;
        let slot = self
            .trees
            .get(idx.index())
            .and_then(Option::as_ref)
            .ok_or(ModelError::UnknownAttackTree(id))?;
        slot.residual
            .as_ref()
            .map(TreeEvalCache::evaluation)
            .map_err(Clone::clone)
    }

    /// Returns the cached inherent [`Evaluation`] of the attack tree linked
    /// to threat scenario `id`.
    ///
    /// Time O(log T + log K), space O(1).
    ///
    /// # Errors
    ///
    /// Same as [`Project::evaluate_threat`].
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
    ///     id: threat, name: "Spoof".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?,
    ///     step_feasibility: vec![(step, high)], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_threat(&project, threat)?.root_value(), &high);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_threat(
        &self,
        project: &Project,
        id: ThreatId,
    ) -> Result<&Evaluation<FeasibilityLevel>, ModelError> {
        let t = project.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = project
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.evaluate_tree(project, tree_id)
    }

    /// Returns the cached residual [`Evaluation`] of the attack tree linked
    /// to threat scenario `id`.
    ///
    /// Time O(log T + log K), space O(1).
    ///
    /// # Errors
    ///
    /// Same as [`Project::evaluate_residual_threat`].
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
    ///     id: threat, name: "Spoof".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (low, high) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("high").ok_or("no level")?,
    /// );
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
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
    ///     tree: tree_id, node: step, control: ctrl, feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_residual_threat(&project, threat)?.root_value(), &low);
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_residual_threat(
        &self,
        project: &Project,
        id: ThreatId,
    ) -> Result<&Evaluation<FeasibilityLevel>, ModelError> {
        let t = project.threat_ref(id)?;
        let tree_idx = t.attack_tree.ok_or(ModelError::NoAttackTree(id))?;
        let tree_id = project
            .trees
            .id(tree_idx)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        self.evaluate_residual_tree(project, tree_id)
    }

    /// Returns the cached inherent [`ThreatRisk`] of threat scenario `id`.
    ///
    /// Time O(log T) (public ID lookup + slot borrow), space O(1).
    ///
    /// # Errors
    ///
    /// Same as [`Project::evaluate_risk`].
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId,
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
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// project.apply(Op::SetThreatFeasibility {
    ///     threat, level: Some(high), provenance: Provenance::default(),
    /// })?;
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_risk(&project, threat)?.max_risk(), m.risk_level("r5"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_risk(
        &self,
        project: &Project,
        id: ThreatId,
    ) -> Result<&ThreatRisk, ModelError> {
        let idx = project
            .threat_scenario_idx(&id)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        let slot = self
            .threats
            .get(idx.index())
            .and_then(Option::as_ref)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        slot.inherent.as_ref().map_err(Clone::clone)
    }

    /// Returns the cached residual [`ThreatRisk`] of threat scenario `id`.
    ///
    /// Time O(log T) (public ID lookup + slot borrow), space O(1).
    ///
    /// # Errors
    ///
    /// Same as [`Project::evaluate_residual_risk`].
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
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
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
    /// let cache = project.eval_cache();
    /// assert_eq!(cache.evaluate_residual_risk(&project, threat)?.max_risk(), m.risk_level("r2"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn evaluate_residual_risk(
        &self,
        project: &Project,
        id: ThreatId,
    ) -> Result<&ThreatRisk, ModelError> {
        let idx = project
            .threat_scenario_idx(&id)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        let slot = self
            .threats
            .get(idx.index())
            .and_then(Option::as_ref)
            .ok_or(ModelError::UnknownThreatScenario(id))?;
        slot.residual.as_ref().map_err(Clone::clone)
    }

    /// Returns the cumulative number of attack-tree nodes (steps + gates)
    /// recomputed by incremental [`update`](Self::update) calls since the
    /// cache was built or last reset with [`reset_counters`](Self::reset_counters).
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ProjectEvalCache};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(m);
    /// let cache = ProjectEvalCache::new(&project);
    /// assert_eq!(cache.recomputed_nodes(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn recomputed_nodes(&self) -> u64 {
        self.recomputed_nodes
    }

    /// Returns the cumulative number of threat-risk evaluations (inherent +
    /// residual) recomputed by incremental [`update`](Self::update) calls
    /// since the cache was built or last reset with
    /// [`reset_counters`](Self::reset_counters). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ProjectEvalCache};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(m);
    /// let cache = ProjectEvalCache::new(&project);
    /// assert_eq!(cache.recomputed_threats(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn recomputed_threats(&self) -> u64 {
        self.recomputed_threats
    }

    /// Resets [`recomputed_nodes`](Self::recomputed_nodes) and
    /// [`recomputed_threats`](Self::recomputed_threats) to zero. Time O(1),
    /// space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{Methodology, Project, ProjectEvalCache};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let project = Project::new(m);
    /// let mut cache = ProjectEvalCache::new(&project);
    /// cache.reset_counters();
    /// assert_eq!(cache.recomputed_nodes(), 0);
    /// assert_eq!(cache.recomputed_threats(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn reset_counters(&mut self) {
        self.recomputed_nodes = 0;
        self.recomputed_threats = 0;
    }

    fn arenas_compatible(&self, project: &Project, op: &Op) -> bool {
        let trees_ok = match op {
            Op::AddAttackTree { .. } | Op::RemoveAttackTree { .. } => {
                project.trees.slot_count() <= self.trees.len().saturating_add(1)
            }
            _ => self.trees.len() == project.trees.slot_count(),
        };
        let threats_ok = match op {
            Op::AddThreatScenario { .. } | Op::RemoveThreatScenario { .. } => {
                project.threats.slot_count() <= self.threats.len().saturating_add(1)
            }
            _ => self.threats.len() == project.threats.slot_count(),
        };
        let damages_ok = match op {
            Op::AddDamageScenario { .. } | Op::RemoveDamageScenario { .. } => {
                project.damages.slot_count() <= self.damage_threats.len().saturating_add(1)
            }
            _ => self.damage_threats.len() == project.damages.slot_count(),
        };
        trees_ok && threats_ok && damages_ok && self.tree_threats.len() == self.trees.len()
    }

    fn build_tree_eval(project: &Project, entry: &TreeEntry) -> CachedTreeEval {
        let domain = MaxMin::<FeasibilityLevel>::new();
        let inherent = TreeEvalCache::new(&entry.tree, &domain, |idx| {
            entry.effective_step_feasibility(&project.methodology, idx)
        });
        let residual = TreeEvalCache::new(&entry.tree, &domain, |idx| {
            project.entry_residual_step_feasibility(entry, idx)
        });
        CachedTreeEval { inherent, residual }
    }

    fn build_threat_eval(
        &self,
        project: &Project,
        threat_id: ThreatId,
        t: &ThreatScenario,
    ) -> CachedThreatEval {
        let inherent = self.eval_threat_mode(project, threat_id, t, false);
        let residual = self.eval_threat_mode(project, threat_id, t, true);
        CachedThreatEval { inherent, residual }
    }

    fn eval_threat_mode(
        &self,
        project: &Project,
        threat_id: ThreatId,
        t: &ThreatScenario,
        residual: bool,
    ) -> Result<ThreatRisk, ModelError> {
        if let Some(tree_idx) = t.attack_tree {
            let slot = self
                .trees
                .get(tree_idx.index())
                .and_then(Option::as_ref)
                .ok_or(ModelError::UnknownThreatScenario(threat_id))?;
            let tree_res = if residual {
                &slot.residual
            } else {
                &slot.inherent
            };
            match tree_res {
                Ok(tc) => {
                    let eval = tc.evaluation().clone();
                    let feas = *eval.root_value();
                    Ok(project.risk_from_feasibility(t, feas, Some(eval)))
                }
                Err(e) => Err(e.clone()),
            }
        } else {
            let level = t
                .direct_feasibility
                .ok_or(ModelError::NoAttackTree(threat_id))?;
            Ok(project.risk_from_feasibility(t, level, None))
        }
    }

    fn update_tree_added(&mut self, project: &Project, tree_idx: TreeIdx) {
        self.trees.resize_with(project.trees.slot_count(), || None);
        self.tree_threats
            .resize_with(project.trees.slot_count(), Vec::new);
        if let Some(entry) = project.trees.get(tree_idx) {
            let n = u64::try_from(entry.tree.len()).unwrap_or(u64::MAX);
            self.recomputed_nodes = self.recomputed_nodes.saturating_add(n.saturating_mul(2));
            if let Some(slot) = self.trees.get_mut(tree_idx.index()) {
                *slot = Some(Self::build_tree_eval(project, entry));
            }
        }
    }

    /// The shape of a tree changed: indices moved or a gate changed meaning,
    /// so this tree's evaluation is rebuilt (O(V + E)) and the threats that
    /// use it are refreshed from it. Other trees and threats are untouched.
    fn update_tree_structure(&mut self, project: &Project, tree: TreeId) {
        let Some(tree_idx) = project.attack_tree_idx(&tree) else {
            return;
        };
        self.update_tree_added(project, tree_idx);
        let linked = self
            .tree_threats
            .get(tree_idx.index())
            .cloned()
            .unwrap_or_default();
        for threat_idx in linked {
            for residual in [false, true] {
                self.sync_threat_from_tree(project, threat_idx, tree_idx, residual, &[], true);
            }
        }
    }

    fn update_tree_removed(&mut self, project: &Project) {
        for (i, slot) in self.trees.iter_mut().enumerate() {
            if slot.is_some()
                && project
                    .trees
                    .slots
                    .get(i)
                    .and_then(Option::as_ref)
                    .is_none()
            {
                *slot = None;
                if let Some(list) = self.tree_threats.get_mut(i) {
                    list.clear();
                }
            }
        }
    }

    fn update_step(
        &mut self,
        project: &Project,
        tree_id: TreeId,
        node_id: NodeId,
        include_inherent: bool,
    ) {
        let Some(tree_idx) = project.attack_tree_idx(&tree_id) else {
            return;
        };
        let Some(entry) = project.trees.get(tree_idx) else {
            return;
        };
        let Some(node_idx) = entry.tree.idx_of(&node_id) else {
            return;
        };
        if include_inherent {
            self.update_tree_mode(project, tree_idx, entry, &[node_idx], false);
        }
        self.update_tree_mode(project, tree_idx, entry, &[node_idx], true);
    }

    fn update_control_target(&mut self, project: &Project, target: ItemRef) {
        let Ok(target_idx) = project.item_idx(target) else {
            return;
        };
        for (tree_idx, _, entry) in project.trees.iter() {
            let dirty: Vec<NodeIdx> = entry
                .vias()
                .filter(|&(i, via)| {
                    entry.attack_type_at(i).is_some()
                        && (via == target_idx
                            || project.item_owner_component(via) == Some(target_idx))
                })
                .filter_map(|(i, _)| NodeIdx::try_from_usize(i))
                .collect();
            if !dirty.is_empty() {
                self.update_tree_mode(project, tree_idx, entry, &dirty, true);
            }
        }
    }

    fn update_control_rule(
        &mut self,
        project: &Project,
        control: ControlId,
        attack_type: AttackType,
    ) {
        let Some(ctrl_idx) = project.security_control_idx(&control) else {
            return;
        };
        let Some(ctrl) = project.controls.get(ctrl_idx) else {
            return;
        };
        if ctrl.targets.is_empty() {
            return;
        }
        for (tree_idx, _, entry) in project.trees.iter() {
            let dirty: Vec<NodeIdx> = entry
                .vias()
                .filter(|&(i, via)| {
                    entry.attack_type_at(i) == Some(attack_type)
                        && (ctrl.targets.contains(&via)
                            || project
                                .item_owner_component(via)
                                .is_some_and(|o| ctrl.targets.contains(&o)))
                })
                .filter_map(|(i, _)| NodeIdx::try_from_usize(i))
                .collect();
            if !dirty.is_empty() {
                self.update_tree_mode(project, tree_idx, entry, &dirty, true);
            }
        }
    }

    fn update_tree_mode(
        &mut self,
        project: &Project,
        tree_idx: TreeIdx,
        entry: &TreeEntry,
        node_indices: &[NodeIdx],
        residual: bool,
    ) {
        let domain = MaxMin::<FeasibilityLevel>::new();
        let all_rated = node_indices.iter().all(|&node_idx| {
            if residual {
                project
                    .entry_residual_step_feasibility(entry, node_idx)
                    .is_some()
            } else {
                entry
                    .effective_step_feasibility(&project.methodology, node_idx)
                    .is_some()
            }
        });
        let mut touched = Vec::new();

        let Some(Some(cached_tree)) = self.trees.get_mut(tree_idx.index()) else {
            self.rebuild(project);
            return;
        };
        let tree_res = if residual {
            &mut cached_tree.residual
        } else {
            &mut cached_tree.inherent
        };

        let (full_rebuild, tree_changed) = if let (true, Ok(tc)) = (all_rated, tree_res.as_mut()) {
            let res = if residual {
                tc.recompute_steps_recording(
                    &entry.tree,
                    &domain,
                    node_indices.iter().copied(),
                    |i| project.entry_residual_step_feasibility(entry, i),
                    &mut touched,
                )
            } else {
                tc.recompute_steps_recording(
                    &entry.tree,
                    &domain,
                    node_indices.iter().copied(),
                    |i| entry.effective_step_feasibility(&project.methodology, i),
                    &mut touched,
                )
            };
            match res {
                Ok((count, rebuilt)) => {
                    self.recomputed_nodes = self
                        .recomputed_nodes
                        .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
                    (rebuilt, rebuilt || !touched.is_empty())
                }
                Err(err) => {
                    *tree_res = Err(err);
                    (true, true)
                }
            }
        } else {
            let n = u64::try_from(entry.tree.len()).unwrap_or(u64::MAX);
            self.recomputed_nodes = self.recomputed_nodes.saturating_add(n);
            *tree_res = if residual {
                TreeEvalCache::new(&entry.tree, &domain, |i| {
                    project.entry_residual_step_feasibility(entry, i)
                })
            } else {
                TreeEvalCache::new(&entry.tree, &domain, |i| {
                    entry.effective_step_feasibility(&project.methodology, i)
                })
            };
            (true, true)
        };

        if !tree_changed {
            return;
        }

        let Some(linked_threats) = self.tree_threats.get(tree_idx.index()).cloned() else {
            return;
        };
        for threat_idx in linked_threats {
            self.sync_threat_from_tree(
                project,
                threat_idx,
                tree_idx,
                residual,
                &touched,
                full_rebuild,
            );
        }
    }

    fn sync_threat_from_tree(
        &mut self,
        project: &Project,
        threat_idx: ThreatIdx,
        tree_idx: TreeIdx,
        residual: bool,
        touched: &[NodeIdx],
        full_rebuild: bool,
    ) {
        let (Some(threat_id), Some(t)) = (
            project.threats.id(threat_idx),
            project.threats.get(threat_idx),
        ) else {
            return;
        };
        if !full_rebuild {
            let tree_eval = self
                .trees
                .get(tree_idx.index())
                .and_then(Option::as_ref)
                .and_then(|ct| {
                    if residual {
                        ct.residual.as_ref().ok()
                    } else {
                        ct.inherent.as_ref().ok()
                    }
                })
                .map(TreeEvalCache::evaluation);
            if let Some(src_eval) = tree_eval {
                let new_feas = *src_eval.root_value();
                if let Some(Some(threat_slot)) = self.threats.get_mut(threat_idx.index()) {
                    let target = if residual {
                        &mut threat_slot.residual
                    } else {
                        &mut threat_slot.inherent
                    };
                    if let Ok(risk) = target.as_mut() {
                        match risk.tree_evaluation.as_mut() {
                            Some(dst_eval) => dst_eval.sync_nodes(src_eval, touched),
                            None => risk.tree_evaluation = Some(src_eval.clone()),
                        }
                        if risk.feasibility != new_feas {
                            risk.feasibility = new_feas;
                            project.update_risk_entries(t, risk);
                        }
                        self.recomputed_threats = self.recomputed_threats.saturating_add(1);
                        return;
                    }
                }
            }
        }

        let new_res = self.eval_threat_mode(project, threat_id, t, residual);
        if let Some(Some(threat_slot)) = self.threats.get_mut(threat_idx.index()) {
            if residual {
                threat_slot.residual = new_res;
            } else {
                threat_slot.inherent = new_res;
            }
            self.recomputed_threats = self.recomputed_threats.saturating_add(1);
        }
    }

    fn update_damage_impact(&mut self, project: &Project, damage_id: DamageId) {
        let Some(damage_idx) = project.damage_scenario_idx(&damage_id) else {
            return;
        };
        let Some(linked_threats) = self.damage_threats.get(damage_idx.index()).cloned() else {
            return;
        };
        for threat_idx in linked_threats {
            self.refresh_threat_entries(project, threat_idx);
        }
    }

    fn refresh_threat_entries(&mut self, project: &Project, threat_idx: ThreatIdx) {
        let Some(t) = project.threats.get(threat_idx) else {
            return;
        };
        let Some(Some(slot)) = self.threats.get_mut(threat_idx.index()) else {
            return;
        };
        if let Ok(risk) = slot.inherent.as_mut() {
            project.update_risk_entries(t, risk);
            self.recomputed_threats = self.recomputed_threats.saturating_add(1);
        }
        if let Ok(risk) = slot.residual.as_mut() {
            project.update_risk_entries(t, risk);
            self.recomputed_threats = self.recomputed_threats.saturating_add(1);
        }
    }

    fn update_threat_added(&mut self, project: &Project, threat_idx: ThreatIdx, id: ThreatId) {
        self.threats
            .resize_with(project.threats.slot_count(), || None);
        let Some(t) = project.threats.get(threat_idx) else {
            return;
        };
        if let Some(tree_idx) = t.attack_tree
            && let Some(list) = self.tree_threats.get_mut(tree_idx.index())
        {
            insert_sorted_unique(list, threat_idx);
        }
        for &damage_idx in &t.damages {
            if let Some(list) = self.damage_threats.get_mut(damage_idx.index()) {
                insert_sorted_unique(list, threat_idx);
            }
        }
        let eval = self.build_threat_eval(project, id, t);
        if let Some(slot) = self.threats.get_mut(threat_idx.index()) {
            *slot = Some(eval);
            self.recomputed_threats = self.recomputed_threats.saturating_add(2);
        }
    }

    fn update_threat_removed(&mut self, project: &Project) {
        for (i, slot) in self.threats.iter_mut().enumerate() {
            if slot.is_some()
                && project
                    .threats
                    .slots
                    .get(i)
                    .and_then(Option::as_ref)
                    .is_none()
            {
                *slot = None;
                if let Some(threat_idx) = u32::try_from(i).ok().map(ThreatIdx::from_u32) {
                    for list in &mut self.tree_threats {
                        remove_sorted(list, threat_idx);
                    }
                    for list in &mut self.damage_threats {
                        remove_sorted(list, threat_idx);
                    }
                }
            }
        }
    }

    fn update_threat_feasibility(&mut self, project: &Project, threat_id: ThreatId) {
        let Some(threat_idx) = project.threat_scenario_idx(&threat_id) else {
            return;
        };
        let Some(t) = project.threats.get(threat_idx) else {
            return;
        };
        if t.attack_tree.is_some() {
            // Direct feasibility is shadowed while a tree is linked.
            return;
        }
        let eval = self.build_threat_eval(project, threat_id, t);
        if let Some(slot) = self.threats.get_mut(threat_idx.index()) {
            *slot = Some(eval);
            self.recomputed_threats = self.recomputed_threats.saturating_add(2);
        }
    }

    fn update_threat_tree(&mut self, project: &Project, threat_id: ThreatId) {
        let Some(threat_idx) = project.threat_scenario_idx(&threat_id) else {
            return;
        };
        let Some(t) = project.threats.get(threat_idx) else {
            return;
        };
        for (i, list) in self.tree_threats.iter_mut().enumerate() {
            if t.attack_tree.map(TreeIdx::index) == Some(i) {
                insert_sorted_unique(list, threat_idx);
            } else {
                remove_sorted(list, threat_idx);
            }
        }
        let eval = self.build_threat_eval(project, threat_id, t);
        if let Some(slot) = self.threats.get_mut(threat_idx.index()) {
            *slot = Some(eval);
            self.recomputed_threats = self.recomputed_threats.saturating_add(2);
        }
    }

    fn update_threat_damages(&mut self, project: &Project, threat_id: ThreatId) {
        let Some(threat_idx) = project.threat_scenario_idx(&threat_id) else {
            return;
        };
        let Some(t) = project.threats.get(threat_idx) else {
            return;
        };
        for (i, list) in self.damage_threats.iter_mut().enumerate() {
            if t.damages.iter().any(|d| d.index() == i) {
                insert_sorted_unique(list, threat_idx);
            } else {
                remove_sorted(list, threat_idx);
            }
        }
        self.refresh_threat_entries(project, threat_idx);
    }
}

fn insert_sorted_unique(list: &mut Vec<ThreatIdx>, idx: ThreatIdx) {
    if let Err(pos) = list.binary_search_by_key(&idx.index(), |x| x.index()) {
        list.insert(pos, idx);
    }
}

fn remove_sorted(list: &mut Vec<ThreatIdx>, idx: ThreatIdx) {
    if let Ok(pos) = list.binary_search_by_key(&idx.index(), |x| x.index()) {
        list.remove(pos);
    }
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

    #[test]
    #[allow(clippy::too_many_lines)]
    fn attack_potential_derives_and_overrides_step_feasibility() {
        use crate::GateKind;
        let meth = testkit::builtin();
        let mut proj = Project::new(Arc::clone(&meth));
        let asset = testkit::asset_id(1);
        let dmg = testkit::damage_id(2);
        let threat = testkit::threat_id(3);
        let tr = testkit::tree_id(4);
        let (root_id, s1_id, s2_id) = (
            testkit::node_id(5),
            testkit::node_id(6),
            testkit::node_id(7),
        );
        let safety = testkit::cat(&meth, "safety");
        let severe = testkit::lvl(&meth, safety, "severe");
        let low = testkit::feas(&meth, "low");
        let high = testkit::feas(&meth, "high");
        let medium = testkit::feas(&meth, "medium");
        let very_low = testkit::feas(&meth, "very-low");

        proj.apply(Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: meth.all_properties(),
            provenance: Provenance::default(),
        })
        .expect("asset");
        proj.apply(Op::AddDamageScenario {
            id: dmg,
            name: "Crash".into(),
            impacts: vec![(safety, severe)],
            provenance: Provenance::default(),
        })
        .expect("damage");
        let mut tree =
            AttackTree::new(root_id, crate::tree::Node::gate(GateKind::And, "Goal")).expect("root");
        tree.add_child(tree.root(), s1_id, crate::tree::Node::step("Step 1"))
            .expect("s1");
        tree.add_child(tree.root(), s2_id, crate::tree::Node::step("Step 2"))
            .expect("s2");
        proj.apply(Op::AddAttackTree {
            id: tr,
            tree,
            step_feasibility: vec![(s2_id, high)],
            provenance: Provenance::default(),
        })
        .expect("tree");
        proj.apply(Op::AddThreatScenario {
            id: threat,
            name: "Tamper".into(),
            asset,
            category: testkit::threat_cat(&meth, "tampering"),
            catalog_refs: vec![],
            damages: vec![dmg],
            provenance: Provenance::default(),
        })
        .expect("threat");
        proj.apply(Op::SetThreatAttackTree {
            threat,
            tree: Some(tr),
            provenance: Provenance::default(),
        })
        .expect("link tree");

        let tr_idx = proj.attack_tree_idx(&tr).expect("tree idx");
        let tree_ref = proj.attack_tree(tr_idx).expect("tree");
        let root_idx = tree_ref.root();
        let s1_idx = tree_ref.idx_of(&s1_id).expect("s1 idx");

        // Validation errors on SetStepFactor.
        let et = testkit::factor(&meth, "elapsed-time");
        let le_1w = testkit::factor_lvl(&meth, et, "le-1w");
        assert_eq!(
            proj.validate(&Op::SetStepFactor {
                tree: testkit::tree_id(99),
                node: s1_id,
                factor: et,
                level: Some(le_1w),
                provenance: Provenance::default(),
            }),
            Err(ModelError::UnknownAttackTree(testkit::tree_id(99)))
        );
        assert_eq!(
            proj.validate(&Op::SetStepFactor {
                tree: tr,
                node: testkit::node_id(99),
                factor: et,
                level: Some(le_1w),
                provenance: Provenance::default(),
            }),
            Err(ModelError::UnknownTreeNode {
                tree: tr,
                node: testkit::node_id(99),
            })
        );
        assert_eq!(
            proj.validate(&Op::SetStepFactor {
                tree: tr,
                node: root_id,
                factor: et,
                level: Some(le_1w),
                provenance: Provenance::default(),
            }),
            Err(ModelError::NotAStep {
                tree: tr,
                node: root_id,
            })
        );
        let bad_factor = FeasibilityFactor::new(9);
        assert_eq!(
            proj.validate(&Op::SetStepFactor {
                tree: tr,
                node: s1_id,
                factor: bad_factor,
                level: None,
                provenance: Provenance::default(),
            }),
            Err(ModelError::UnknownFeasibilityFactor(bad_factor))
        );
        let bad_level = FactorLevel::new(9);
        assert_eq!(
            proj.validate(&Op::SetStepFactor {
                tree: tr,
                node: s1_id,
                factor: et,
                level: Some(bad_level),
                provenance: Provenance::default(),
            }),
            Err(ModelError::UnknownFactorLevel {
                factor: et,
                level: bad_level,
            })
        );
        assert_eq!(proj.step_factor(tr_idx, root_idx, et), None);

        // Partial factor rating: AttackTreeStepIncompleteFactors + AttackTreeStepUnrated.
        let undo_et = proj
            .apply(Op::SetStepFactor {
                tree: tr,
                node: s1_id,
                factor: et,
                level: Some(le_1w),
                provenance: Provenance::default(),
            })
            .expect("rate et");
        assert_eq!(proj.step_factor(tr_idx, s1_idx, et), Some(le_1w));
        assert_eq!(proj.step_attack_potential(tr_idx, s1_idx), None);
        assert_eq!(proj.effective_step_feasibility(tr_idx, s1_idx), None);
        let findings = proj.lint();
        assert!(findings.iter().any(|f| {
            f.code()
                == LintCode::AttackTreeStepIncompleteFactors {
                    tree: tr,
                    node: s1_id,
                }
        }));
        assert!(findings.iter().any(|f| {
            f.code()
                == LintCode::AttackTreeStepUnrated {
                    tree: tr,
                    node: s1_id,
                }
        }));

        // Cannot remove attack tree while factors are rated.
        let unlinked = proj
            .apply(Op::SetThreatAttackTree {
                threat,
                tree: None,
                provenance: Provenance::default(),
            })
            .expect("unlink");
        assert_eq!(
            proj.validate(&Op::RemoveAttackTree { id: tr }),
            Err(ModelError::AttackTreeHasFactors {
                tree: tr,
                factors: 1,
            })
        );
        proj.apply(unlinked).expect("relink");

        // Rate remaining 4 factors on s1: points = 1 + 6 + 3 + 4 + 4 = 18 -> medium (14..=19).
        for (fk, lk) in [
            ("expertise", "expert"),
            ("knowledge", "restricted"),
            ("window-of-opportunity", "moderate"),
            ("equipment", "specialized"),
        ] {
            let fac = testkit::factor(&meth, fk);
            let lvl = testkit::factor_lvl(&meth, fac, lk);
            proj.apply(Op::SetStepFactor {
                tree: tr,
                node: s1_id,
                factor: fac,
                level: Some(lvl),
                provenance: Provenance::default(),
            })
            .expect("rate factor");
        }
        let ap = proj
            .step_attack_potential(tr_idx, s1_idx)
            .expect("complete ap");
        assert_eq!(ap.score(), 18);
        assert_eq!(ap.feasibility(), medium);
        assert_eq!(
            proj.effective_step_feasibility(tr_idx, s1_idx),
            Some(medium)
        );
        // AND(medium, high) = medium.
        assert_eq!(*proj.evaluate_tree(tr).expect("eval").root_value(), medium);
        let findings = proj.lint();
        assert!(!findings.iter().any(|f| {
            matches!(
                f.code(),
                LintCode::AttackTreeStepUnrated { .. }
                    | LintCode::AttackTreeStepIncompleteFactors { .. }
                    | LintCode::RedundantStepFeasibility { .. }
            )
        }));

        // Setting coarse step_feasibility on s1 is overridden by complete factors
        // and triggers RedundantStepFeasibility.
        let before_coarse = proj.clone();
        proj.apply(Op::SetStepFeasibility {
            tree: tr,
            node: s1_id,
            level: Some(low),
            provenance: Provenance::default(),
        })
        .expect("coarse");
        assert_eq!(proj.step_feasibility(tr_idx, s1_idx), Some(low));
        assert_eq!(
            proj.effective_step_feasibility(tr_idx, s1_idx),
            Some(medium)
        );
        assert!(proj.lint().iter().any(|f| {
            f.code()
                == LintCode::RedundantStepFeasibility {
                    tree: tr,
                    node: s1_id,
                }
        }));
        let diff = before_coarse.diff(&proj).expect("diff");
        assert_eq!(diff.attack_trees().modified().len(), 1);

        // Security control mitigation caps the derived inherent feasibility.
        let cid = testkit::control_id(20);
        proj.apply(Op::AddSecurityControl {
            id: cid,
            name: "HSM".into(),
            provenance: Provenance::default(),
        })
        .expect("control");
        proj.apply(Op::SetStepMitigation {
            tree: tr,
            node: s1_id,
            control: cid,
            feasibility: Some(very_low),
            provenance: Provenance::default(),
        })
        .expect("mitigation");
        assert_eq!(
            *proj
                .evaluate_residual_tree(tr)
                .expect("res eval")
                .root_value(),
            very_low
        );
        assert_eq!(
            proj.evaluate_residual_risk(threat)
                .expect("res risk")
                .feasibility(),
            very_low
        );

        // Clearing one factor reverts effective_step_feasibility to coarse `low`.
        proj.apply(undo_et).expect("undo et");
        assert_eq!(proj.step_attack_potential(tr_idx, s1_idx), None);
        assert_eq!(proj.effective_step_feasibility(tr_idx, s1_idx), Some(low));
    }

    #[test]
    fn check_invariants_detects_bad_step_factors() {
        use crate::GateKind;
        let m = testkit::builtin();
        let mut p = Project::new(Arc::clone(&m));
        let tr = testkit::tree_id(1);
        let (root_id, s1_id) = (testkit::node_id(2), testkit::node_id(3));
        let mut tree =
            AttackTree::new(root_id, crate::tree::Node::gate(GateKind::Or, "Goal")).expect("root");
        tree.add_child(tree.root(), s1_id, crate::tree::Node::step("Step 1"))
            .expect("s1");
        p.apply(Op::AddAttackTree {
            id: tr,
            tree,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        })
        .expect("tree");
        assert_eq!(p.check_invariants(), Ok(()));

        // Non-canonical all-None factor slice.
        p.trees.slots[0].as_mut().expect("live").step_factors[1] = vec![None; 5].into_boxed_slice();
        assert!(p.check_invariants().is_err());

        // Factor ratings on a gate node.
        p.trees.slots[0].as_mut().expect("live").step_factors[1] = Box::default();
        p.trees.slots[0].as_mut().expect("live").step_factors[0] =
            vec![Some(FactorLevel::new(0)), None, None, None, None].into_boxed_slice();
        assert!(p.check_invariants().is_err());

        // Out-of-range factor level on a step node.
        p.trees.slots[0].as_mut().expect("live").step_factors[0] = Box::default();
        p.trees.slots[0].as_mut().expect("live").step_factors[1] =
            vec![Some(FactorLevel::new(99)), None, None, None, None].into_boxed_slice();
        assert!(p.check_invariants().is_err());

        p.trees.slots[0].as_mut().expect("live").step_factors[1] = Box::default();
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
