# ADR 0012: Threat-scenario links to attack trees and feasibility evaluation

- Status: proposed (job 013-threat-attack-trees)
- Date: 2026-10-23
- Crate: `crates/aegis-model` (`src/id.rs`, `src/tree.rs`,
  `src/methodology/`, `src/threat.rs`, `src/op.rs`, `src/project.rs`,
  `src/error.rs`, `src/store/`, `packs/iso21434-r155.json`)
- Rules: P3, P4, P5, D2, D3, D4, D5, D7, D8, D9, D10, D12, R3, R6, A8, A9,
  Q3, Q4, Q5, Q6, L1 in `docs/engineering-standards.md`
- Builds on: `0001-arena-and-typed-ids.md`, `0004-tree-evaluation-and-why.md`,
  `0008-methodology-neutral-domain-model.md`, `0009-save-load-format.md`,
  `0011-methodology-pack-files.md`

## Context

In the prior jobs, the attack-tree engine (`AttackTree`, `evaluate`,
`Explanation`, `why_path`) and the TARA project core (`Project` with
assets, damage scenarios, and threat scenarios over a `Methodology`) lived
side by side without a link between them. In an actual TARA, a threat
scenario's feasibility comes from evaluating the attack tree that realizes
it, and the analyst's explain-why trace comes from that evaluation's
decisive path.

Three design questions had to be settled with the owner (recorded in
`factory/jobs/013-threat-attack-trees/rework.md`):

1. Whether an attack tree is embedded inside one `ThreatScenario` or lives
   in its own `TreeId`/`TreeIdx` arena on `Project` so multiple threats can
   reference the same tree.
2. How step feasibility is represented in a methodology-neutral way in v0.
3. Which operations and persistence changes belong in this job vs. a
   follow-up store job.

## Options

### 1. Tree ownership: embedded per threat vs. first-class `TreeId`/`TreeIdx` arena

| Option | Good | Bad |
|---|---|---|
| A. Embedded `Option<AttackTree>` on `ThreatScenario` | No fourth entity kind or reference count | Two threats that share an attack path (e.g., two assets on the same bus reached via the same gateway compromise) must duplicate the tree and keep ratings in sync by hand |
| **B. Separate `TreeId`/`TreeIdx` arena on `Project`; `ThreatScenario` holds `Option<TreeIdx>`** | Follows the exact `Arena<T, Id, Idx>` + `RefCounts` pattern of assets and damage scenarios; multiple threats can share a tree; `RemoveAttackTree` checks `refs.trees` in O(1) (`AttackTreeInUse`) | One more arena, ID pair, and reference-count vector |

**Chosen: B (confirmed by owner).**

### 2. Feasibility scale in v0

| Option | Good | Bad |
|---|---|---|
| **A. Ordered `feasibility_levels: Vec<LevelDef>` (`2..=16`, lowest to highest) on `MethodologyDef`, indexed by `FeasibilityLevel(u8)`, evaluated with `MaxMin<FeasibilityLevel>`** | Methodology-neutral; reuses `LevelDef` and `MaxMin<T>`; O(1) index validation; OR picks highest feasibility (easiest branch for the attacker), AND/SEQ_AND picks lowest (bottleneck step) | Multi-factor attack potential (ISO/SAE 21434 Annex G: elapsed time, expertise, knowledge, window, equipment -> sum -> level) is a later extension on top of leaf ratings |
| B. Fixed numeric score (`u16`) or elapsed-time only (`MinTime`) | No methodology vocabulary change | Not methodology-neutral; loses the methodology's named scale (`very-low` .. `high`) |

**Chosen: A (confirmed by owner).** Because `FeasibilityLevel(u8)` is
ordered from lowest feasibility (`0`, hardest for the attacker) to highest
feasibility (`len - 1`, easiest for the attacker), `MaxMin<FeasibilityLevel>`
Directly implements standard gate semantics:
- **OR gate**: `max` over children (an attacker only needs one branch, so
  the gate is as feasible as its most feasible child).
- **AND / SEQ_AND gate**: `min` over children (an attacker must complete
  every child, so the gate is limited by its least feasible child).

And `Evaluation::explain` + `Evaluation::why_path` trace the decisive
children down to the bottleneck or easiest leaf steps with no changes to
`eval.rs`.

### 3. Operations and persistence scope

Confirmed by owner:
- **Operations**: 4 new `Op` variants (`AddAttackTree`,
  `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`), bringing
  `Op` from 16 to 20 variants. Each has an exact inverse op for O(op size)
  undo (D9).
- **Pack format v1**: `feasibility_levels` is added to `MethodologyDef` and
  pack v1 now (`packs/iso21434-r155.json` declares `very-low`, `low`,
  `medium`, `high`; `tests/fixtures/packs/med-device-demo.json` declares
  `improbable`, `remote`, `probable`).
- **Snapshot and op-log store**: Wire encoding of attack trees in snapshots
  and the 4 new ops in the op log is deferred to a follow-up store job so
  this job stays focused on the domain model and existing v1 golden files
  load unchanged. Rather than silently dropping data, `Project::save`
  returns `StoreError::UnencodableOp` when `project.attack_tree_count() > 0`
  before writing any bytes, and `OpLogWriter::append` returns
  `StoreError::UnencodableOp` on the 4 new ops before advancing `seq` or
  writing to the sink.

## Decision

### Identifiers and storage

- `TreeIdx(u32)` and `TreeId(Uuid)` (UUIDv7) join `AssetIdx`/`AssetId`,
  `DamageIdx`/`DamageId`, and `ThreatIdx`/`ThreatId` in `src/id.rs`.
- `Project` stores `trees: Arena<TreeEntry, TreeId, TreeIdx>`, where:
  ```rust
  struct TreeEntry {
      tree: AttackTree,
      step_feasibility: Box<[Option<FeasibilityLevel>]>,
  }
  ```
  `step_feasibility` is indexed by `NodeIdx::index()` (`len == tree.len()`).
  Gate slots are always `None`; step slots are `Some(level)` when rated and
  `None` when unrated (unrated is never conflated with the lowest level).
- `ThreatScenario` gains `attack_tree: Option<TreeIdx>` (public getter
  `ThreatScenario::attack_tree(&self) -> Option<TreeIdx>`).
- `RefCounts` gains `trees: Vec<u32>`, tracking how many live threat
  scenarios reference each tree slot.

### Validation and invariants

`Op::AddAttackTree { id, tree, step_feasibility }` validates in
O(V + E + r log r + log n) before mutating the project:
1. `id` is not already in `self.trees` (`DuplicateAttackTree`).
2. `self.trees.slot_count() < self.slot_limit` (`ProjectCapacityExceeded`).
3. Every node's label passes `check_name` (non-blank after trim, at most
   `MAX_NAME_LEN` = 256 chars).
4. `tree.topological_order()` succeeds (defense in depth against cycles).
5. Every `(node_id, level)` in `step_feasibility` has a valid
   `FeasibilityLevel` in the project's methodology
   (`InvalidFeasibilityLevel`), resolves to a node in `tree`
   (`UnknownNodeId`), targets a `Node::Step` rather than a `Node::Gate`
   (`GateHasFeasibility`), and appears at most once (`DuplicateStepRating`,
   checked by sorting node indices in O(r log r)).

`Op::RemoveAttackTree { id }` requires `refs.trees[idx] == 0`
(`AttackTreeInUse`) and returns `Op::AddAttackTree` with the restored
`AttackTree` and its `step_feasibility` ratings in `NodeIdx` order so
applying the inverse restores the exact tree and ratings.

### Evaluation

- `Project::evaluate_tree(&self, id: TreeId) -> Result<Evaluation<FeasibilityLevel>, ModelError>`
  looks up the live tree and calls
  `evaluate(&entry.tree, &MaxMin::<FeasibilityLevel>::new(), |idx| entry.step_feasibility[idx.index()])`.
  Any unrated step returns `ModelError::MissingLeafValue(node_idx)`, and an
  empty gate returns `ModelError::EmptyGate(node_idx)`.
- `Project::evaluate_threat(&self, id: ThreatId) -> Result<Evaluation<FeasibilityLevel>, ModelError>`
  resolves the threat, returns `ModelError::NoAttackTree(id)` if
  `threat.attack_tree` is `None`, and otherwise evaluates the linked tree.

## Complexity

n = entities of the kind, V = nodes in the tree, E = edges in the tree,
r = step ratings supplied in `AddAttackTree`.

| Operation | Time | Space |
|---|---|---|
| `AddAttackTree` | O(log n + V + E + r log r) | O(V + r) |
| `SetStepFeasibility` | O(log n) | O(1) |
| `SetThreatAttackTree` | O(log n) | O(1) |
| `RemoveAttackTree` | O(log n + V + E) | O(V) |
| `evaluate_tree` | O(log n + V + E) | O(V) |
| `evaluate_threat` | O(log n + V + E) | O(V) |

## Numbers

Measured on aarch64-apple-darwin:

| Item | Value |
|---|---|
| `TreeIdx` / `TreeId` | 4 / 16 bytes |
| `FeasibilityLevel` / `Option<FeasibilityLevel>` | 1 / 2 bytes |
| `ThreatScenario` (and `Option<ThreatScenario>`) | 80 bytes (was 72) |
| Built-in pack (`packs/iso21434-r155.json`) | 4,094 bytes |
| Maximal pack (`max_def()`, 705 entries) | 1,507,728 bytes (1.44 MiB, under 4 MiB cap) |

## Consequences

- Threat scenarios now link to attack trees and derive their feasibility
  and explain-why path through the methodology's `FeasibilityLevel` scale.
- All 20 `Op` variants have exact inverses verified by property tests over
  both the built-in ISO/SAE 21434 + UN R155 methodology and the second
  medical-device test methodology.
- Snapshot and op-log persistence of attack trees is tracked as a follow-up
  store job; until then, `Project::save` and `OpLogWriter::append` fail
  explicitly with `StoreError::UnencodableOp` rather than silently omitting
  trees or ratings.
