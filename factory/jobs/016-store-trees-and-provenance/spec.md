# 016-store-trees-and-provenance: Store persistence for attack trees and provenance

Builds on jobs 010 (`docs/adr/0009-save-load-format.md`), 013 (`docs/adr/0012-threat-attack-trees.md`), and 014 (`docs/adr/0013-element-provenance.md`). Jobs 013 and 014 added `AttackTree` entities, step feasibility ratings, threat-to-tree links, the four tree operations (`AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`), and `Provenance` on every element and creating/mutating operation, deferring wire persistence to this job.

## Goal

Persist attack trees, step feasibility ratings, threat-to-tree links, the four attack tree operations, and non-default `Provenance` in `aegis-project` snapshots and `aegis-oplog` operation logs:
- Every valid `Project` and every valid `Op` (all 20 variants, with any valid `Provenance`) round-trips through `Project::save`/`Project::load` and `OpLogWriter::append`/`OpLogReader`.
- Existing `aegis-project` and `aegis-oplog` v1 files (`tests/fixtures/project-v1.json` and `tests/fixtures/oplog-v1.jsonl`) load and resave byte-for-byte identically by omitting optional fields when empty or default (`attack_trees` when empty, `attack_tree` when `None`, `provenance` when `Provenance::is_default()`).
- Every variable-length string and list is capped at decode time before `Project::validate` runs (R3, AC11/AC12 of ADR 0009), and a model limit `MAX_ATTACK_TREE_NODES = 32` bounds project attack trees so every valid `Op::AddAttackTree` fits within `MAX_OP_RECORD_BYTES` (64 KiB).
- Stored files never contain in-memory indices (`NodeIdx`, `TreeIdx`, `FeasibilityLevel(u8)`): tree nodes are referenced by `NodeId` UUIDs and feasibility levels by stable methodology string keys.

## Scope

In (`crates/aegis-model`):
- Model limit `MAX_ATTACK_TREE_NODES: usize = 32` in `src/tree.rs` (re-exported at the crate root) and `ModelError::TooManyAttackTreeNodes { tree: TreeId, max: usize }` checked in `Project::validate` (`check_add_tree`). Standalone `AttackTree` values outside `Project` remain bounded only by `u32::MAX`.
- `AttackTree::from_wire_parts` and `AttackTree::linked_parents_of` in `src/tree.rs` to reconstruct and inspect `AttackTree` values in O(V + E) time and space while enforcing all structural invariants (`AttackTree::check_invariants` plus `owner.index() < i`).
- Store decode caps in `src/store/mod.rs`:
  - `MAX_ACTOR_BYTES: usize = 4 * MAX_ACTOR_CHARS` (1,024 bytes, re-exported at the crate root)
  - `Limit::ActorBytes { max: usize }`
  - `Limit::AttackTreeNodes { max: usize }`
  - `Limit::StepFeasibility { max: usize }`
  - `VocabKind::FeasibilityLevel` (`"feasibility level"`)
- Wire types in `src/store/wire.rs`, `src/store/snapshot.rs`, and `src/store/op_log.rs`:
  - `ProvenanceView` / `ProvenanceWire` (`actor`, `timestamp_ms`, `source`: `"human" | "import" | "catalog" | "ai"`), omitted on write when `Provenance::is_default()` and rejected on read if an explicit `"provenance"` object is the default triple `("", 0, "human")`.
  - `NodesView` / `NodesWire` (`id`, `label`, `gate`: `"and" | "or" | "seq_and"`, `children`, and `parents` when a node has linked parents).
  - `StepFeasibilityView` / `StepFeasibilityWire` (`node`, `level` resolved via `FeasibilityLevelKey`).
  - `AttackTreeView` / `AttackTreeWire` in `SnapshotView` / `SnapshotWire`, with `"attack_trees"` omitted on write when `project.attack_tree_count() == 0` and defaulted to `[]` on read.
  - `attack_tree: Option<TreeId>` on `ThreatView` / `ThreatWire`, omitted on write when `None`.
  - All 4 tree operations (`add_attack_tree`, `set_step_feasibility`, `set_threat_attack_tree`, `remove_attack_tree`) and `provenance` on all 16 creating/mutating `Op` variants in `OpView` / `OpWire`.
- Tests (`tests/store.rs`, `tests/store_limits.rs`, `tests/store_properties.rs`, `tests/common/mod.rs`, `tests/project.rs`), memory benchmark cases (`benches/store_memory.rs`), and ADR 0014 (`docs/adr/0014-store-trees-and-provenance.md`).

Out:
- Cryptographic hash chaining of the op log (reserved for a future format version).
- Per-node provenance inside `AttackTree`.

## Acceptance criteria

- **AC1 (Snapshot round-trip for trees and provenance):** A `Project` with attack trees (including shared DAG nodes via `AttackTree::link`), step feasibility ratings, threat-to-tree links, and non-default `Provenance` across all 4 `ProvenanceSource` variants on assets, damage scenarios, threat scenarios, and attack trees round-trips through `Project::save` and `Project::load` with `loaded == original`.
- **AC2 (Op log round-trip for all 20 ops and provenance):** All 20 `Op` variants (including `AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`, and their undo inverses) with default and non-default `Provenance` round-trip through `OpLogWriter::append` and `OpLogReader` with `decoded == original`, and replaying the log produces a `Project` equal to the live project.
- **AC3 (Backward compatibility with v1 golden fixtures):** `tests/fixtures/project-v1.json` and `tests/fixtures/oplog-v1.jsonl` load and re-encode byte-for-byte identically.
- **AC4 (Canonical determinism and key-only storage):** `Project::save` output is byte-for-byte identical across different edit/undo histories and independent of methodology vocabulary ordering (including `feasibility_levels` order). No in-memory indices (`NodeIdx`, `TreeIdx`, `FeasibilityLevel(u8)`) appear on disk.
- **AC5 (Decode caps and cap parity):** `Limit::ActorBytes`, `Limit::AttackTreeNodes`, and `Limit::StepFeasibility` fire at decode time before model validation, `ModelError::TooManyAttackTreeNodes` rejects trees with `> MAX_ATTACK_TREE_NODES` (32) nodes in `Project::apply`, and a project at every model limit at once (including a 32-node attack tree with 256-char labels, 64-byte feasibility keys, and 256-char actor provenance) saves, loads, and logs within `MAX_OP_RECORD_BYTES`.
- **AC6 (Memory and engineering standards):** All cases in `benches/store_memory.rs` (including new attack-tree cases) pass within `SERVER_BUDGET` (`3.0x`), and `./scripts/ci.sh`, `cargo test --benches`, and `./scripts/deny.sh` pass with zero warnings.
