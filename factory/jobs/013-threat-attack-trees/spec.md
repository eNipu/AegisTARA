# 013-threat-attack-trees: Link threat scenarios to attack trees and evaluate threat feasibility in the project's methodology

Builds on `docs/adr/0001-arena-and-typed-ids.md`, `docs/adr/0002-dag-links-and-cycle-detection.md`, `docs/adr/0005-tree-evaluation.md`, `docs/adr/0006-explain-why-traces.md`, `docs/adr/0008-methodology-neutral-domain-model.md`, and `docs/adr/0011-methodology-pack-files.md`. Until now `crates/aegis-model/` held attack trees (`AttackTree`, `evaluate`, `Explanation`) and the TARA core (`Methodology`, `Project`, `Asset`, `DamageScenario`, `ThreatScenario`, `Op`) side by side without connecting them. This job connects the two halves.

## Open questions

Three big-picture questions. Each has a default. If the owner says nothing, the builder implements the default.

1. **Are attack trees first-class project entities or embedded 1:1 inside each threat scenario?**
   **Default: first-class project entities in their own `Project` arena (`TreeId`, `TreeIdx`), referenced by `ThreatScenario` via `attack_tree: Option<TreeIdx>`.** A threat scenario starts with `None` when cataloged, links to an attack tree when analyzed (`Op::SetThreatAttackTree`), and can unlink it without destroying the tree. Multiple threat scenarios may reference the same tree when one multi-step attack path realizes threats on several assets or categories, and `RefCounts` prevents removing a tree while any live threat references it (`ModelError::AttackTreeInUse`). The alternative is to embed an `Option<AttackTree>` directly inside each `ThreatScenario` (1:1 ownership, no `TreeId`/`TreeIdx`, no sharing across threats).

2. **How are leaf steps rated in the project's methodology in v0: direct ordinal `FeasibilityLevel` per step, or multi-factor scoring tables (attack potential / attack vector)?**
   **Default: each methodology declares an ordered feasibility scale (`feasibility_levels: Vec<LevelDef>`, 2 to 16 levels, lowest feasibility first, indexed by `FeasibilityLevel(u8)`), each leaf step holds an optional `FeasibilityLevel`, and gates combine child levels with `MaxMin<FeasibilityLevel>` (`OR` = max = most feasible child, `AND` = `SAND` = min = least feasible bottleneck step).** This keeps the core model methodology-neutral (ADR 0008): ISO/SAE 21434 (`very-low`, `low`, `medium`, `high`), medical-device scales, and custom packs all map step ratings onto an ordered feasibility scale, and `evaluate` from jobs 005 and 006 computes both the threat's feasibility and its explain-why trace without new evaluator code. Factor-based step scoring (for example ISO/SAE 21434 Annex G five-factor attack potential tables) belongs to a later pack/engine job that populates or derives a step's `FeasibilityLevel`. The alternative is to add factor definitions and scoring tables to `MethodologyDef` in this job as well.

3. **How do operations (`Op`) edit attack trees in a `Project`, and does this job extend the `aegis-project` / `aegis-oplog` wire formats?**
   **Default: add four operations (`AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`) where `AddAttackTree` attaches a constructed `AttackTree` with initial step ratings and `SetStepFeasibility` updates or clears one step's rating in O(1); update the methodology pack format for `feasibility_levels` now, and defer snapshot/op-log wire encoding of attack trees to a follow-up store job (refusing `Project::save` when `attack_tree_count() > 0` and `OpLogWriter::append` on the four new ops with `StoreError::UnencodableOp`, so nothing is silently dropped and the golden v1 project/oplog fixtures stay unchanged).** `AttackTree` is currently append-only (`new`, `add_child`, `link`, no node tombstones or `unlink`); tree-level `AddAttackTree` / `RemoveAttackTree` plus O(1) `SetStepFeasibility` and `SetThreatAttackTree` gives every `Op` an exact single-op inverse (`apply(inverse)` restores an `==` project) without needing node tombstones inside `AttackTree` yet. The alternative is to add per-node `Op` variants (`AddTreeNode`, `RemoveTreeNode`, `LinkTreeNode`, `UnlinkTreeNode`) and v2 snapshot/op-log wire serialization in this job.

Small choices settled here, which the owner does not need to decide:
- **Hot step ratings live in a `NodeIdx`-indexed array per tree (`Vec<Option<FeasibilityLevel>>`), separate from `Node` (D4).** `AttackTree` and `Node` stay domain-independent (`Node::Step` carries only `label: Arc<str>`). In `Project`, each `TreeEntry` holds `tree: AttackTree` and `step_feasibility: Vec<Option<FeasibilityLevel>>` of length `tree.len()` (`2` bytes per node). Evaluating feasibility reads only the structure and this 2-byte-per-node slice.
- **Unrated steps fail evaluation with `ModelError::MissingLeafValue(NodeId)`, and gates cannot carry step ratings (`ModelError::NotAStep`).** Evaluation never substitutes a default rating for an unrated step (ADR 0005).
- **`Op::AddThreatScenario` keeps its existing six fields.** A newly added threat starts with `attack_tree: None`; `Op::SetThreatAttackTree` links, relinks, or unlinks its tree. Removing a threat while it still references a tree fails with `ModelError::ThreatHasAttackTree { threat, tree }`, so `RemoveThreatScenario`'s single-op inverse remains `AddThreatScenario` with no information loss and no churn across existing `AddThreatScenario` call sites.
- **No new dependencies (L6).**

## Goal

A methodology declares its ordered feasibility scale (`feasibility_levels`, lowest to highest), a `Project` holds `AttackTree`s with per-step `FeasibilityLevel` ratings validated against its `Methodology`, and each `ThreatScenario` can reference the `AttackTree` that realizes it (`attack_tree() -> Option<TreeIdx>`). Calling `project.evaluate_threat(threat_id)` (or `project.evaluate_tree(tree_id)`) evaluates that tree with `MaxMin<FeasibilityLevel>` in one O(V + E) pass and returns an `Evaluation<FeasibilityLevel>` carrying the threat's root `FeasibilityLevel`, every node's derived `FeasibilityLevel`, and the explain-why trace (`derivation`, `explain`, `why_path` down to the deciding leaf step). Every mutation goes through `Project::apply` (`AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`) with `validate` agreement, referential integrity (`AttackTreeInUse`, `ThreatHasAttackTree`), and an exact inverse op for undo (D9).

## Scope

In:
- `TreeIdx` (`u32`) and `TreeId` (`UUIDv7`) in `crates/aegis-model/src/id.rs` via `entity_ids!` (D1, D2, D3).
- `PartialEq` and `Eq` on `AttackTree` in `crates/aegis-model/src/tree.rs`.
- `FeasibilityLevel` (`u8` vocabulary index via `vocab_index!`), `MethodologyDef.feasibility_levels: Vec<LevelDef>` (2 to 16 levels, lowest feasibility first), validation in `Methodology::new`, and lookup/iteration methods on `Methodology` in `crates/aegis-model/src/methodology/mod.rs`.
- Pack reader and writer support for `feasibility_levels` in `crates/aegis-model/src/methodology/pack.rs`, plus the four ISO/SAE 21434 feasibility levels (`very-low`, `low`, `medium`, `high`) in `crates/aegis-model/packs/iso21434-r155.json` and five levels in `crates/aegis-model/tests/fixtures/packs/med-device-demo.json`.
- `ThreatScenario::attack_tree(&self) -> Option<TreeIdx>` in `crates/aegis-model/src/threat.rs`.
- Four new `Op` variants in `crates/aegis-model/src/op.rs`: `AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`.
- Attack-tree arena, step-feasibility side array (D4), reference counts (`RefCounts.trees`), read accessors, `evaluate_tree`, `evaluate_threat`, validation, commit, and invariant checking in `crates/aegis-model/src/project.rs`.
- New `MethodologyError` and `ModelError` variants in `crates/aegis-model/src/error.rs`.
- Explicit `StoreError::UnencodableOp` refusal in `crates/aegis-model/src/store/snapshot.rs` (when `attack_tree_count() > 0`) and `crates/aegis-model/src/store/op_log.rs` (for the four new ops).
- Unit tests, integration tests in `crates/aegis-model/tests/project.rs`, updated 20-op property suite in `crates/aegis-model/tests/ops_properties.rs` and `crates/aegis-model/tests/common/mod.rs`, doctests, and ADR `docs/adr/0012-threat-attack-trees.md` (L1).

Out:
- Factor-based step scoring (attack potential factors, CVSS-like attack vectors, lookup tables).
- Risk matrices combining a damage scenario's impact with a threat scenario's feasibility (`risk = f(impact, feasibility)`), risk treatment decisions, cybersecurity goals, and countermeasures.
- Fine-grained node-editing ops (`AddTreeNode`, `RemoveTreeNode`, `LinkTreeNode`, `UnlinkTreeNode`) and node tombstones inside `AttackTree`.
- Snapshot and op-log wire serialization of attack trees and the four new ops (follow-up store job).
- Incremental tree recompute (A1), caching `Evaluation<FeasibilityLevel>` across edits, and UI or report rendering.

## Design notes

### Files

| File | Change |
|---|---|
| `crates/aegis-model/src/id.rs` | `TreeIdx` and `TreeId` in `entity_ids!`; unit tests |
| `crates/aegis-model/src/tree.rs` | derive `PartialEq, Eq` on `AttackTree`; unit test |
| `crates/aegis-model/src/methodology/mod.rs` | `FeasibilityLevel`, `MethodologyDef.feasibility_levels`, validation, accessors, `pub(crate) fn check_feasibility_level`, unit tests |
| `crates/aegis-model/src/methodology/pack.rs` | `feasibility_levels` in `DefWire`, `From<DefWire>`, and `DefView` (capped by `LevelsCap`) |
| `crates/aegis-model/packs/iso21434-r155.json` | add `"feasibility_levels"` (`very-low`, `low`, `medium`, `high`) |
| `crates/aegis-model/src/threat.rs` | `attack_tree: Option<TreeIdx>` field and `ThreatScenario::attack_tree` accessor |
| `crates/aegis-model/src/op.rs` | `AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree` variants and complexity table |
| `crates/aegis-model/src/error.rs` | 2 `MethodologyError` variants and 9 `ModelError` variants |
| `crates/aegis-model/src/project.rs` | `TreeEntry`, `trees` arena, `RefCounts.trees`, tree/step accessors, `evaluate_tree`, `evaluate_threat`, op validation & commit, `PartialEq`, `check_invariants` |
| `crates/aegis-model/src/store/snapshot.rs` | refuse `save` with `StoreError::UnencodableOp` when `attack_tree_count() > 0` |
| `crates/aegis-model/src/store/op_log.rs` | `within_caps` returns `false` for the 4 new `Op` variants |
| `crates/aegis-model/src/lib.rs` | re-export `FeasibilityLevel`, `TreeId`, `TreeIdx`; update crate docs and ADR list |
| `crates/aegis-model/src/methodology/catalog.rs`, `crates/aegis-model/src/store/mod.rs` | add `feasibility_levels` to `MethodologyDef` doctest literals if any |
| `crates/aegis-model/tests/common/mod.rs` | add `feasibility_levels` to `second_def()` and `wide()`, `tree_id`/`node_id`/`feas` helpers, extend `Gen::op` to all 20 ops while `accepted` uses the 16 v1 store ops, update `describe` and `second_every_op` |
| `crates/aegis-model/tests/fixtures/packs/med-device-demo.json` | add `"feasibility_levels"` matching `second_def()` |
| `crates/aegis-model/tests/project.rs` | tests for threat-to-tree linking, feasibility evaluation, explain-why traces, referential integrity, undo, and error redaction |
| `crates/aegis-model/tests/ops_properties.rs` | assert all 20 `Op` kinds succeed and `>= 28` error kinds occur |
| `crates/aegis-model/tests/builtin_pack.rs`, `crates/aegis-model/tests/pack.rs`, `crates/aegis-model/tests/pack_limits.rs`, `crates/aegis-model/tests/pack_properties.rs`, `crates/aegis-model/tests/second_methodology.rs`, `crates/aegis-model/tests/store_limits.rs`, `crates/aegis-model/benches/pack.rs`, `crates/aegis-model/benches/store_memory.rs` | include `feasibility_levels` in `MethodologyDef` test/bench literals and `expected()` |
| `docs/adr/0012-threat-attack-trees.md` | new ADR |
| `docs/adr/0008-methodology-neutral-domain-model.md`, `docs/adr/0011-methodology-pack-files.md` | short pointers to ADR 0012 |

### Methodology feasibility scale (`src/methodology/mod.rs`, `src/methodology/pack.rs`)

Add `FeasibilityLevel` to `vocab_index!`:

```rust
vocab_index! {
    ...
    /// Index of a feasibility level inside a [`Methodology`], ordered from
    /// lowest (`0`) to highest feasibility.
    FeasibilityLevel,
}
```

Because `vocab_index!` derives `PartialOrd, Ord` on the inner `u8` and `feasibility_levels` is declared from lowest (`0`) to highest (`n - 1`) feasibility, `FeasibilityLevel` is ordered from lowest to highest feasibility. `MaxMin<FeasibilityLevel>` from `src/eval.rs` therefore takes `max` (most feasible child) at `GateKind::Or` and `min` (least feasible child) at `GateKind::And` and `GateKind::SeqAnd` with no change to `src/eval.rs`.

In `MethodologyDef`:

```rust
pub struct MethodologyDef {
    pub key: Arc<str>,
    pub version: Arc<str>,
    pub label: Arc<str>,
    pub impact_categories: Vec<ImpactCategoryDef>,
    /// Ordered scale of attack feasibility levels, lowest feasibility
    /// first. Must hold between [`MIN_LEVELS`] (2) and [`MAX_LEVELS`] (16)
    /// levels with unique keys.
    pub feasibility_levels: Vec<LevelDef>,
    pub properties: Vec<PropertyDef>,
    pub threat_categories: Vec<ThreatCategoryDef>,
    pub catalog_schemes: Vec<CatalogSchemeDef>,
}
```

Validation in `Methodology::new` (checked right after `impact_categories`, before `properties`):
- `feasibility_levels.len()` must be in `MIN_LEVELS..=MAX_LEVELS` (`2..=16`), else `MethodologyError::FeasibilityLevelCount { min: MIN_LEVELS, max: MAX_LEVELS }`.
- Each `LevelDef` in `feasibility_levels` is checked with `add_entry(&l.key, &l.label, pos, &mut by_key, MethodologyError::DuplicateFeasibilityLevel)` (`check_key`, `check_label`, uniqueness).

Read methods on `Methodology`:

```rust
/// Iterates the feasibility scale from lowest (`0`) to highest feasibility:
/// `(index, key, label)`. Time O(1) per item.
pub fn feasibility_levels(
    &self,
) -> impl ExactSizeIterator<Item = (FeasibilityLevel, &str, &str)> + '_;

/// Resolves a feasibility level key in O(log L), L <= 16.
#[must_use]
pub fn feasibility_level(&self, key: &str) -> Option<FeasibilityLevel>;

/// Returns the key of `level` in O(1), or `None` if `level` is outside this
/// methodology's feasibility scale.
#[must_use]
pub fn feasibility_level_key(&self, level: FeasibilityLevel) -> Option<&str>;

/// Returns the human-readable label of `level` in O(1), or `None` if
/// `level` is outside this methodology's feasibility scale.
#[must_use]
pub fn feasibility_level_label(&self, level: FeasibilityLevel) -> Option<&str>;

pub(crate) fn check_feasibility_level(&self, level: FeasibilityLevel) -> Result<(), ModelError>;
```

Pack format (`src/methodology/pack.rs`, `packs/iso21434-r155.json`, `tests/fixtures/packs/med-device-demo.json`):
- `DefWire` gains `feasibility_levels: BoundedVec<Object<EntryWire>, LevelsCap>` right after `impact_categories`. It reuses the existing `LevelsCap` (`MAX_LEVELS = 16`, `Limit::LEVELS`) and `EntryWire` (`Key` + `Label`), so decode-time caps apply to every feasibility level key, label, and the list length before `Methodology::new` runs (R3, R6).
- `DefView` serializes `feasibility_levels` right after `impact_categories` (`serialize_struct("MethodologyDef", 8)`).
- `crates/aegis-model/packs/iso21434-r155.json` defines:
  `[{"key": "very-low", "label": "Very low"}, {"key": "low", "label": "Low"}, {"key": "medium", "label": "Medium"}, {"key": "high", "label": "High"}]`.
- `crates/aegis-model/tests/fixtures/packs/med-device-demo.json` and `second_def()` in `crates/aegis-model/tests/common/mod.rs` define a 5-level scale:
  `improbable` ("Improbable"), `remote` ("Remote"), `occasional` ("Occasional"), `probable` ("Probable"), `frequent` ("Frequent").
- Both pack files must be byte-for-byte canonical (`MethodologyDef::write_pack`).

### Threat scenarios, attack trees, and step ratings (`src/threat.rs`, `src/project.rs`)

In `ThreatScenario` (`src/threat.rs`):

```rust
pub struct ThreatScenario {
    pub(crate) name: Arc<str>,
    pub(crate) asset: AssetIdx,
    pub(crate) category: ThreatCategory,
    pub(crate) catalog_refs: Vec<CatalogRef>,
    pub(crate) damages: Vec<DamageIdx>,
    pub(crate) attack_tree: Option<TreeIdx>,
}

impl ThreatScenario {
    /// Returns the attack tree that realizes this threat scenario, if
    /// linked. Time O(1).
    #[must_use]
    pub fn attack_tree(&self) -> Option<TreeIdx> {
        self.attack_tree
    }
}
```

In `Project` (`src/project.rs`):

```rust
/// One attack tree inside a [`Project`]: the graph and its step
/// feasibility ratings.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TreeEntry {
    tree: AttackTree,
    /// Indexed by `NodeIdx::index()`, length `tree.len()`. `Some(level)` on
    /// a rated `Node::Step`; `None` on an unrated step and on every
    /// `Node::Gate` (D4: hot 2-byte slots separate from cold labels).
    step_feasibility: Vec<Option<FeasibilityLevel>>,
}
```

- `Project` gains `trees: Arena<TreeId, TreeIdx, TreeEntry>` and `RefCounts` gains `trees: Vec<u32>` (live threats referencing each tree slot, 4 bytes per tree slot).
- Read accessors on `Project`:

```rust
/// Returns a live attack tree. Time O(1).
#[must_use]
pub fn attack_tree(&self, idx: TreeIdx) -> Option<&AttackTree>;

/// Resolves an attack tree's public ID. Time O(log n).
#[must_use]
pub fn attack_tree_idx(&self, id: &TreeId) -> Option<TreeIdx>;

/// Returns a live attack tree's public ID. Time O(1).
#[must_use]
pub fn attack_tree_id(&self, idx: TreeIdx) -> Option<TreeId>;

/// Returns the number of live attack trees. Time O(1).
#[must_use]
pub fn attack_tree_count(&self) -> usize;

/// Iterates live attack trees in public-ID order. Time O(1) per item,
/// O(n) for the whole walk; space O(1).
pub fn attack_trees(&self) -> impl Iterator<Item = (TreeIdx, TreeId, &AttackTree)>;

/// Returns the feasibility rating of step `node` in `tree`, or `None` if
/// `tree` is not live, `node` is not in `tree`, `node` is a gate, or the
/// step is unrated. Time O(1).
#[must_use]
pub fn step_feasibility(&self, tree: TreeIdx, node: NodeIdx) -> Option<FeasibilityLevel>;

/// Returns the `NodeIdx`-indexed step feasibility slice of a live attack
/// tree (length `tree.len()`, `None` on gates and unrated steps). Time O(1).
#[must_use]
pub fn step_feasibilities(&self, tree: TreeIdx) -> Option<&[Option<FeasibilityLevel>]>;
```

### Feasibility evaluation and explain-why (`src/project.rs`)

```rust
/// Evaluates a live attack tree's feasibility in the project's methodology
/// using [`MaxMin<FeasibilityLevel>`](crate::MaxMin) (`OR` = max = most
/// feasible child, `AND` = `SAND` = min = least feasible child).
///
/// The returned [`Evaluation<FeasibilityLevel>`](crate::Evaluation) is
/// derived data (D10): it carries [`Evaluation::root_value`], per-node
/// values via [`Evaluation::get`], and the explain-why trace via
/// [`Evaluation::derivation`], [`Evaluation::explain`], and
/// [`Evaluation::why_path`].
///
/// Time O(log n + V + E), extra space O(V), where n is `attack_tree_count()`
/// and V, E are the tree's nodes and edges.
///
/// # Errors
///
/// - [`ModelError::UnknownAttackTree`] if `id` is not in the project.
/// - [`ModelError::EmptyGate`] if a gate in the tree has no children.
/// - [`ModelError::MissingLeafValue`] if a step in the tree has no
///   feasibility rating.
/// - [`ModelError::Cycle`] if the tree contains a cycle (defense in depth).
pub fn evaluate_tree(&self, id: TreeId) -> Result<Evaluation<FeasibilityLevel>, ModelError>;

/// Evaluates the feasibility of threat scenario `id` from its linked attack
/// tree and explain-why trace in the project's methodology.
///
/// Resolves `id` to its linked attack tree and delegates to the same single
/// O(V + E) pass as [`evaluate_tree`](Self::evaluate_tree).
///
/// Time O(log n + V + E), extra space O(V).
///
/// # Errors
///
/// - [`ModelError::UnknownThreatScenario`] if `id` is not in the project.
/// - [`ModelError::NoAttackTree`] if the threat scenario has no linked
///   attack tree.
/// - [`ModelError::EmptyGate`], [`ModelError::MissingLeafValue`], or
///   [`ModelError::Cycle`] from evaluating the linked tree.
pub fn evaluate_threat(&self, id: ThreatId) -> Result<Evaluation<FeasibilityLevel>, ModelError>;
```

Implementation of `evaluate_tree`:
1. Look up `let (_, entry) = self.live_tree(id)?`.
2. Call `evaluate(&entry.tree, &MaxMin::<FeasibilityLevel>::new(), |idx| entry.step_feasibility.get(idx.index()).copied().flatten())`.

### Operations (`src/op.rs`, `src/project.rs`, `src/error.rs`)

Four new variants on `Op`:

```rust
    /// Adds an attack tree and initial step feasibility ratings. Inverse:
    /// `RemoveAttackTree`.
    AddAttackTree {
        /// Public ID of the new attack tree.
        id: TreeId,
        /// The attack tree graph. Every node label must be a valid name,
        /// and the graph must be acyclic.
        tree: AttackTree,
        /// Initial feasibility ratings per step node. Duplicates, gate
        /// nodes, unknown node IDs, and levels outside the methodology are
        /// rejected.
        step_feasibility: Vec<(NodeId, FeasibilityLevel)>,
    },
    /// Sets or clears (`None`) one step's feasibility rating in an attack
    /// tree. Inverse: `SetStepFeasibility` with the previous rating.
    SetStepFeasibility {
        /// The attack tree.
        tree: TreeId,
        /// The step node in `tree`.
        node: NodeId,
        /// The new feasibility level, or `None` to clear the rating.
        level: Option<FeasibilityLevel>,
    },
    /// Links a threat scenario to the attack tree that realizes it, or
    /// unlinks (`None`) its current attack tree. Inverse:
    /// `SetThreatAttackTree` with the previous link.
    SetThreatAttackTree {
        /// The threat scenario.
        threat: ThreatId,
        /// The attack tree to link, or `None` to unlink.
        tree: Option<TreeId>,
    },
    /// Removes an attack tree that no threat scenario references. Inverse:
    /// `AddAttackTree`.
    RemoveAttackTree {
        /// Public ID of the attack tree to remove.
        id: TreeId,
    },
```

Validation order in `Project::validate` (identical to `Project::apply`):
- `AddAttackTree { id, tree, step_feasibility }`:
  1. `self.trees.idx(&id).is_some()` -> `ModelError::DuplicateAttackTree(id)`.
  2. For each `(_, _, node)` in `tree.iter()` in arena order: `check_name(node.label())?` (`EmptyName` or `NameTooLong`).
  3. `tree.topological_order()?` (`Cycle`).
  4. With `seen = vec![false; tree.len()]`, for each `&(node, level)` in `step_feasibility`:
     - `let Some(node_idx) = tree.idx_of(&node)` else `ModelError::UnknownTreeNode { tree: id, node }`;
     - `if !matches!(tree.get(node_idx), Some(Node::Step { .. }))` -> `ModelError::NotAStep { tree: id, node }`;
     - `self.methodology.check_feasibility_level(level)?` -> `ModelError::UnknownFeasibilityLevel(level)`;
     - `let slot = seen.get_mut(node_idx.index())...`: if already `true`, `ModelError::DuplicateStepFeasibility { tree: id, node }`, else set `true`.
  5. `self.trees.next_idx(self.slot_limit).is_none()` -> `ModelError::ProjectCapacityExceeded`.
- `SetStepFeasibility { tree, node, level }`:
  1. `let (_, entry) = self.live_tree(tree)?` -> `ModelError::UnknownAttackTree(tree)`.
  2. `let Some(node_idx) = entry.tree.idx_of(&node)` else `ModelError::UnknownTreeNode { tree, node }`.
  3. `if !matches!(entry.tree.get(node_idx), Some(Node::Step { .. }))` -> `ModelError::NotAStep { tree, node }`.
  4. `if let Some(level) = level { self.methodology.check_feasibility_level(level)?; }` -> `ModelError::UnknownFeasibilityLevel(level)`.
- `SetThreatAttackTree { threat, tree }`:
  1. `self.live_threat(threat)?` -> `ModelError::UnknownThreatScenario(threat)`.
  2. `if let Some(tree) = tree { self.live_tree(tree)?; }` -> `ModelError::UnknownAttackTree(tree)`.
- `RemoveAttackTree { id }`:
  1. `let (idx, _) = self.live_tree(id)?` -> `ModelError::UnknownAttackTree(id)`.
  2. `let threats = self.refs.trees.get(idx.index()).copied().unwrap_or(0); if threats > 0 { return Err(ModelError::AttackTreeInUse { tree: id, threats }); }`.
- `RemoveThreatScenario { id }` (updated check):
  1. `let t = self.threat_ref(id)?` -> `ModelError::UnknownThreatScenario(id)`.
  2. `if let Some(tree_idx) = t.attack_tree { let tree = self.trees.id(tree_idx).ok_or(ModelError::UnknownThreatScenario(id))?; return Err(ModelError::ThreatHasAttackTree { threat: id, tree }); }`.

Commit & inverse rules:
- `commit_add_tree`: builds `slots = vec![None; tree.len()]`, writes `Some(level)` at each rated step's `node_idx.index()`, pushes `TreeEntry { tree, step_feasibility: slots }` to `self.trees` and `0` to `self.refs.trees`, and returns `Op::RemoveAttackTree { id }`.
- `commit_remove_tree`: removes `entry` from `self.trees`, collects `step_feasibility` in node arena order (`entry.tree.iter()` filtering `entry.step_feasibility[idx.index()]` for `Some(level)`), and returns `Op::AddAttackTree { id, tree: entry.tree, step_feasibility }`.
- `commit_set_step_feasibility`: replaces `entry.step_feasibility[node_idx.index()]` with `level` and returns `Op::SetStepFeasibility { tree, node, level: old }`.
- `commit_set_threat_attack_tree`: resolves `new_idx: Option<TreeIdx>`, replaces `t.attack_tree` with `new_idx`, bumps `self.refs.trees` (`+1` for `new_idx` before `-1` for `old_idx`), and returns `Op::SetThreatAttackTree { threat, tree: old_id }`.

New error variants (`src/error.rs`):
- `MethodologyError::FeasibilityLevelCount { min: usize, max: usize }`: `"feasibility_levels has fewer than {min} or more than {max} levels"`
- `MethodologyError::DuplicateFeasibilityLevel(Arc<str>)`: `"duplicate feasibility level key {0:?}"`
- `ModelError::DuplicateAttackTree(TreeId)`: `"attack tree {0} already exists in the project"`
- `ModelError::UnknownAttackTree(TreeId)`: `"attack tree {0} is not in the project"`
- `ModelError::UnknownTreeNode { tree: TreeId, node: NodeId }`: `"node {node} is not in attack tree {tree}"`
- `ModelError::NotAStep { tree: TreeId, node: NodeId }`: `"node {node} of attack tree {tree} is a gate, not a step"`
- `ModelError::UnknownFeasibilityLevel(FeasibilityLevel)`: `"feasibility level index {} is not in the methodology", .0.index()`
- `ModelError::DuplicateStepFeasibility { tree: TreeId, node: NodeId }`: `"step node {node} of attack tree {tree} is rated twice"`
- `ModelError::AttackTreeInUse { tree: TreeId, threats: u32 }`: `"attack tree {tree} is referenced by {threats} threat(s)"`
- `ModelError::ThreatHasAttackTree { threat: ThreatId, tree: TreeId }`: `"threat scenario {threat} still references attack tree {tree}"`
- `ModelError::NoAttackTree(ThreatId)`: `"threat scenario {0} has no linked attack tree"`

### Equality and invariants (`src/project.rs`)

- `PartialEq for Project`: also checks `self.trees.len() == other.trees.len()`, compares live `(TreeId, &TreeEntry)` pairs in `TreeId` order, and in `same_threat` checks `x.attack_tree.and_then(|t| self.trees.id(t)) == y.attack_tree.and_then(|t| other.trees.id(t))` so equality is independent of arena slot assignment.
- `check_invariants` (`#[cfg(test)]`): checks `arena_invariants("tree", &self.trees)`, verifies for every live tree that `step_feasibility.len() == tree.len()`, `tree.topological_order().is_ok()`, every node label passes `check_name`, gate slots in `step_feasibility` are `None`, rated step slots pass `m.check_feasibility_level`, every threat's `attack_tree` points to a live tree slot, and a fresh recount of `trees` across all live threats equals `self.refs.trees`.

### Complexity (A8, A9)

Every new public function and `Op` variant documents its time and space cost:

| Operation / Method | Time | Space |
|---|---|---|
| `Methodology::feasibility_level(key)` | O(log L), L <= 16 | O(1) |
| `Methodology::feasibility_level_key` / `feasibility_level_label` / `feasibility_levels` | O(1) | O(1) |
| `ThreatScenario::attack_tree` | O(1) | O(1) |
| `Project::attack_tree` / `attack_tree_id` / `attack_tree_count` / `step_feasibility` / `step_feasibilities` | O(1) | O(1) |
| `Project::attack_tree_idx` | O(log n), n = live trees | O(1) |
| `Project::attack_trees` | O(1) per item, O(n) total | O(1) |
| `Op::AddAttackTree` | O(log n + V + E + r + text) | O(V) |
| `Op::RemoveAttackTree` | O(log n + V) | O(r), r = rated steps |
| `Op::SetStepFeasibility` | O(log n) | O(1) |
| `Op::SetThreatAttackTree` | O(log n) | O(1) |
| `Project::evaluate_tree` / `Project::evaluate_threat` | O(log n + V + E) | O(V) |

No operation scans the project (`O(project size)` is forbidden, A9).

### ADR (`docs/adr/0012-threat-attack-trees.md`)

Write `docs/adr/0012-threat-attack-trees.md` covering:
- Context: connecting the `AttackTree` / `evaluate` engine (ADRs 0001–0006) with the methodology-neutral `Project` model (ADRs 0008–0011).
- Options and trade-offs for the three Open questions (first-class `TreeId` arena vs embedded 1:1 in `ThreatScenario`; ordinal `FeasibilityLevel` scale with `MaxMin<FeasibilityLevel>` vs multi-factor tables in v0; tree-level add/remove + O(1) step/link ops vs per-node ops).
- D4 layout of `TreeEntry` (`step_feasibility: Vec<Option<FeasibilityLevel>>`, 2 bytes per node, separate from `Node` labels) and why `RemoveThreatScenario` requires unlinking the tree first.
- Complexity table and store impact (`feasibility_levels` in pack v1; `StoreError::UnencodableOp` guard on snapshots with trees and on tree ops until the store v2 job).

## Acceptance criteria

All commands run from `crates/aegis-model/` unless marked "repo root".

- [ ] AC1 `TreeIdx` (`4` bytes) and `TreeId` (`16` bytes, `UUIDv7` validated) exist and round-trip through `Display`/`FromStr`, and `AttackTree` implements `PartialEq` and `Eq`. Test: `cargo test --lib id::tests` and `cargo test --lib tree::tests`.
- [ ] AC2 `MethodologyDef` and `Methodology` validate and expose `feasibility_levels` (`FeasibilityLevel(u8)` ordered from lowest to highest feasibility, `2..=16` levels, valid keys and labels, no duplicate keys), returning `MethodologyError::FeasibilityLevelCount` (< 2 or > 16) and `MethodologyError::DuplicateFeasibilityLevel` on invalid definitions, with key text in errors bounded to at most 64 bytes. Test: `cargo test --lib methodology::tests` and `cargo test --test pack_limits`.
- [ ] AC3 The built-in pack `crates/aegis-model/packs/iso21434-r155.json` declares `very-low < low < medium < high`, `crates/aegis-model/tests/fixtures/packs/med-device-demo.json` declares its 5-level scale (`improbable < remote < occasional < probable < frequent`), and both packs are canonical under `MethodologyDef::write_pack`. Test: `cargo test --test builtin_pack` and `cargo test --test pack`.
- [ ] AC4 `Op::AddAttackTree`, `Op::SetStepFeasibility`, `Op::SetThreatAttackTree`, and `Op::RemoveAttackTree` work over both methodologies, `validate` agrees with `apply` on every valid and invalid case, every accepted op's returned inverse restores an `==` project, and `second_every_op` exercises all 20 `Op` variants. Test: `cargo test --test project` and `cargo test --test second_methodology`.
- [ ] AC5 Referential integrity and validation rules hold:
  - removing an attack tree while a threat scenario references it fails with `ModelError::AttackTreeInUse { tree, threats }` (with the exact referencing threat count, including when two threats share one tree);
  - removing a threat scenario while it still references an attack tree fails with `ModelError::ThreatHasAttackTree { threat, tree }`;
  - rating a gate node fails with `ModelError::NotAStep { tree, node }`;
  - rating an unknown node ID fails with `ModelError::UnknownTreeNode { tree, node }`;
  - duplicate step ratings in `AddAttackTree` fail with `ModelError::DuplicateStepFeasibility { tree, node }`;
  - out-of-range `FeasibilityLevel` indices fail with `ModelError::UnknownFeasibilityLevel(level)`;
  - blank or overlong node labels in `AddAttackTree` fail with `ModelError::EmptyName` / `ModelError::NameTooLong`.
  Test: `cargo test --test project threat_attack_tree_integrity_and_validation`.
- [ ] AC6 Threat feasibility evaluation and explain-why trace:
  - `project.evaluate_threat(threat)` and `project.evaluate_tree(tree)` evaluate a linked DAG attack tree (with `OR`, `AND`, `SeqAnd`, and a shared step via `AttackTree::link`) using `MaxMin<FeasibilityLevel>` in both methodologies (`builtin` and `second`);
  - `eval.root_value()` equals the expected `FeasibilityLevel`, every node's `eval.get(idx)` and `eval.derivation(idx)` match, `eval.explain(tree, idx)` reports the deciding child and inputs, and `eval.why_path(tree.root())` yields the chain of deciding children from the root down to the dominating step;
  - changing a step's rating via `Op::SetStepFeasibility` changes the threat's evaluated feasibility and flips `why_path` to the new dominating branch, and applying the returned inverse op restores the original evaluation and `why_path`;
  - evaluating a threat with no linked tree fails with `ModelError::NoAttackTree(threat)`, an unrated step fails with `ModelError::MissingLeafValue(step_id)`, and an empty gate fails with `ModelError::EmptyGate(gate_id)`.
  Test: `cargo test --test project evaluate_threat_feasibility_and_why_path`.
- [ ] AC7 Property tests (`tests/ops_properties.rs` and `src/project.rs` `project_invariants_*`) cover all 20 `Op` variants and at least 28 `ModelError` variants across 256 random sequences per methodology, checking `validate` == `apply`, single-op undo (`back == before`), full undo back to `Project::new(m)`, and `check_invariants` (including `RefCounts.trees` and `TreeEntry` invariants). Test: `cargo test --test ops_properties` and `cargo test --lib project_invariants`.
- [ ] AC8 Errors never contain customer data (entity names or tree node labels): every new `ModelError` variant is tested with `SENTINEL` in the threat name and gate/step labels, and `!err.to_string().contains(SENTINEL)` holds. Test: `cargo test --test project errors_never_contain_names`.
- [ ] AC9 Store guard and existing golden v1 files: `tests/fixtures/project-v1.json` and `tests/fixtures/oplog-v1.jsonl` load and re-encode byte-for-byte unchanged; `Project::save` on a project with a live attack tree and `OpLogWriter::append` on each of the 4 new `Op` variants return `Err(StoreError::UnencodableOp)`. Test: `cargo test --test store` and `cargo test --test store_properties`.
- [ ] AC10 Neutrality check (repo root, must print nothing):
  `grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'`
- [ ] AC11 No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!`, or `unimplemented!` in library code outside `#[cfg(test)]`, and `#![forbid(unsafe_code)]` stays in place (Q2, Q3). Check (repo root, must print nothing):
  `for f in $(find crates/aegis-model/src -name '*.rs'); do awk '/^#\[cfg\(test\)\]/{exit} {print FILENAME":"FNR": "$0}' "$f"; done | grep -E '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!'`
- [ ] AC12 Every new public item has a doc comment with a compiling example and states its time and space complexity (Q5, A8). Check: `cargo test --doc` and `cargo clippy --all-targets -- -D warnings`.
- [ ] AC13 `docs/adr/0012-threat-attack-trees.md` exists and covers the context, options for the three open questions, D4 layout, complexity table, and store impact; `docs/adr/0008-methodology-neutral-domain-model.md` and `docs/adr/0011-methodology-pack-files.md` link to it. Check (repo root): `test -f docs/adr/0012-threat-attack-trees.md`.
- [ ] AC14 Workspace CI and license policy pass. Check (repo root): `scripts/ci.sh crates/aegis-model` and `scripts/deny.sh licenses bans sources`.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --doc
cargo test --benches
```

From the repo root:

```sh
scripts/ci.sh crates/aegis-model
scripts/deny.sh licenses bans sources
# AC10: must print nothing
grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'
# AC11: must print nothing
for f in $(find crates/aegis-model/src -name '*.rs'); do awk '/^#\[cfg\(test\)\]/{exit} {print FILENAME":"FNR": "$0}' "$f"; done | grep -E '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!'
test -f docs/adr/0012-threat-attack-trees.md
```
