VERDICT: PASS

## Summary

Job `016-store-trees-and-provenance` implements snapshot (`aegis-project` v1) and op-log (`aegis-oplog` v1) persistence for `AttackTree` DAGs, per-step `FeasibilityLevel` ratings, threat-to-tree links, and `Provenance` across all entities and operations, documented in `docs/adr/0014-store-trees-and-provenance.md`.

## Acceptance Criteria Verification

- **AC1 (Backward-compatible v1 golden fixtures)**: Verified in `crates/aegis-model/src/store/snapshot.rs:60-133` and `crates/aegis-model/src/store/op_log.rs:250-502`. `attack_trees` is omitted when empty (`skip_serializing_if = "[_]::is_empty"`), `attack_tree` on threats is omitted when `None`, and `provenance` is omitted when `Provenance::is_default()`. Existing v1 fixtures (`tests/fixtures/project-v1.json`, `tests/fixtures/oplog-v1.jsonl`) remain byte-for-byte unchanged.
- **AC2 (`AttackTree` DAG wire encoding & reconstruction)**: Verified in `crates/aegis-model/src/store/wire.rs:1013-1389` (`AttackTreeWire`, `AttackNodeWire`, `StepFeasibilityMap`) and `crates/aegis-model/src/tree.rs:297-382` (`AttackTree::from_wire_parts`). Pure tree nodes omit `parents`; shared DAG nodes emit `parents: [owner, ...linked_parents]` and are validated against actual incoming edges on load so `owner` (`parents[0].index() < idx.index()`) and `linked_parents` order round-trip exactly.
- **AC3 (`Provenance` wire encoding & canonical omission)**: Verified in `crates/aegis-model/src/store/wire.rs:924-1011` (`ProvenanceWire`). Explicit default provenance (`{"actor":"","timestamp_ms":0,"source":"human"}`) and explicit `null` (`deserialize_with = "some"`) are rejected at decode time so every project and op has a unique canonical wire representation.
- **AC4 (All `Op` variants supported on `aegis-oplog`)**: Verified in `crates/aegis-model/src/store/op_log.rs:250-715`, `tests/store.rs`, and `tests/store_properties.rs`.
- **AC5 (Decode-time caps & `Limit` parity)**: Verified in `crates/aegis-model/src/store/mod.rs:38-188` (`MAX_ACTOR_BYTES = 1024`, `MAX_ATTACK_TREE_NODES = 32`, `Limit::ActorBytes`, `Limit::AttackTreeNodes`, `Limit::StepFeasibility`, `VocabKind::FeasibilityLevel`) and `crates/aegis-model/tests/store_limits.rs:189-269`.
- **AC6 (Model cap `MAX_ATTACK_TREE_NODES = 32`)**: Verified in `crates/aegis-model/src/project.rs:100` and `src/project.rs:2747-2753` (`ModelError::TooManyAttackTreeNodes { tree, max: 32 }`).
- **AC7 (Memory benchmark `attack_trees` in `benches/store_memory.rs`)**: Verified in `crates/aegis-model/benches/store_memory.rs` and `docs/adr/0014-store-trees-and-provenance.md` (peak heap 2.77x, kept heap 1.95x, within budget).

## Engineering Standards Audit

- **D1–D5 (Arenas & Typed IDs)**: `AttackTreeWire` decodes into `AttackTree` arena slots (`NodeIdx`) and `Box<[Option<FeasibilityLevel>]>` via `from_wire_parts`.
- **D7 / P4 (Determinism)**: Snapshot `attack_trees` are sorted by `TreeId`, `step_feasibility` entries are sorted by `NodeId`, and op-log `AddAttackTree` preserves the op's `step_feasibility` vector order for exact `Op` round-tripping.
- **D8 / R6 (Bounded Untrusted Input & No Sensitive Data in Errors)**: All wire strings and arrays enforce `Limit` caps during serde deserialization before allocation or model validation; errors report only IDs, vocabulary kinds, or byte lengths.
- **A8 / A9 (Complexity & No $O(n^2)$)**: `AttackTreeWire::into_parts` resolves `NodeId -> NodeIdx` via a sorted `Vec<(NodeId, NodeIdx)>` and `binary_search_by_key` in $O((V + E + r) \log V)$ time (`src/store/wire.rs:1203-1388`).
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/store/` and `src/tree.rs`.

## Findings

### Blocking
None.

### Non-Blocking
None.
