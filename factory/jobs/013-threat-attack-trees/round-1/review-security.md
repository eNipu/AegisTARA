VERDICT: PASS

Scope: `git diff master...factory/013-threat-attack-trees` (commit 4b765b1), checked against `spec.md`, `rework.md`, `build.md`, Q2–Q4, R3/R4/R6, D5, and D7.

No blocking findings.

Checked and OK:
- `src/methodology/pack.rs`: `feasibility_levels` uses `BoundedVec<LevelWire, LevelsCap>` (`LevelsCap = 16`), so hostile packs with >16 feasibility levels are rejected inside the serde visitor with `Limit::Levels { max: 16 }` before allocating unbounded vectors (R3, R4, R6). Keys and labels on feasibility levels use the existing bounded `Key` (64 B) and `BoundedStr<LabelCap>` (1,024 B) leaves.
- `src/methodology/mod.rs`: `FeasibilityLevelCount` and `DuplicateFeasibilityLevel` validate `2..=16` levels and unique keys; `DuplicateFeasibilityLevel` only stores keys that already passed `check_key` (<= 64 B).
- `src/project.rs`: `AddAttackTree` checks `DuplicateAttackTree` and `slot_limit` (`ProjectCapacityExceeded`) before allocating the rating vector, validates every node label with `check_name` (non-blank, <= 256 chars), verifies `topological_order()` (no cycles), validates every `(node_id, level)` against the methodology (`InvalidFeasibilityLevel`), checks node existence (`UnknownNodeId`), rejects gate ratings (`GateHasFeasibility`), and rejects duplicate node ratings (`DuplicateStepRating`). All fallible checks complete before mutating project state.
- `src/error.rs`: None of the 9 new `ModelError` variants carry entity names or node labels (only typed IDs `TreeId`, `ThreatId`, `NodeId`, and `FeasibilityLevel` index). `errors_never_contain_names` in `tests/project.rs` verifies this across all error variants.
- `src/store/snapshot.rs` & `src/store/op_log.rs`: Rather than silently dropping attack trees or ratings when saving or logging, `Project::save` returns `StoreError::UnencodableOp` when `attack_tree_count() > 0` before writing any bytes, and `OpLogWriter::append` returns `StoreError::UnencodableOp` on the 4 new ops before advancing `seq` or writing to the sink.
- Q2: `#![forbid(unsafe_code)]` remains intact; no `unsafe` added.
- Q3: No `unwrap()`, `expect()`, `panic!`, `todo!`, or `unimplemented!` in library code outside `#[cfg(test)]`.
- D7: Deterministic `BTreeMap` lookups and `NodeIdx`-ordered rating vectors; no `HashMap` added.
