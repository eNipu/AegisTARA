## Round 1
Commits: 4b765b1 Link threat scenarios to attack trees and evaluate feasibility
Files:
- new: `docs/adr/0012-threat-attack-trees.md`
- changed: `crates/aegis-model/src/id.rs` (`TreeIdx`, `TreeId`), `src/tree.rs` (`PartialEq, Eq` on `AttackTree`), `src/methodology/mod.rs` (`FeasibilityLevel`, `MethodologyDef.feasibility_levels`, validation, accessors), `src/methodology/catalog.rs` (test helper), `src/methodology/pack.rs` (`feasibility_levels` in pack v1 wire read/write), `packs/iso21434-r155.json` (`feasibility_levels`), `src/threat.rs` (`attack_tree: Option<TreeIdx>`, `ThreatScenario::attack_tree()`), `src/op.rs` (`AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`), `src/error.rs` (2 `MethodologyError` variants + 9 `ModelError` variants), `src/project.rs` (`TreeEntry`, `trees` arena, `RefCounts.trees`, accessors, `evaluate_tree`, `evaluate_threat`, validation, commit, `PartialEq`, `check_invariants`), `src/store/snapshot.rs` & `src/store/op_log.rs` (`StoreError::UnencodableOp` refusal when attack trees or the 4 new ops are present), `src/lib.rs` (re-exports and docs), `benches/store_memory.rs`, `fuzz/seeds/pack/*.json`, `tests/fixtures/packs/med-device-demo.json`, `tests/common/mod.rs`, `tests/builtin_pack.rs`, `tests/ops_properties.rs`, `tests/pack.rs`, `tests/pack_limits.rs`, `tests/pack_properties.rs`, `tests/project.rs`, `tests/second_methodology.rs`, `tests/store.rs`, `tests/store_limits.rs`, `docs/adr/0008-methodology-neutral-domain-model.md`, `docs/adr/0011-methodology-pack-files.md`

Acceptance criteria:
- [x] AC1 — `TreeId` and `TreeIdx` unit tests and compile-fail doctest pass; sizes are 16 and 4 bytes; `AttackTree` implements `PartialEq, Eq`.
- [x] AC2 — `Methodology` feasibility scale validation (`FeasibilityLevelCount` for `< 2` or `> 16`, `DuplicateFeasibilityLevel`, blank/long labels, invalid keys), `FeasibilityLevel` (`1` byte, `Option<FeasibilityLevel>` `2` bytes), and pack v1 round-trip pass (`builtin_pack`, `pack`, `pack_limits`, `pack_properties`).
- [x] AC3 — `Op::AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, and `RemoveAttackTree` apply and return exact inverse ops; `ThreatScenario::attack_tree()` and `Project` tree accessors work as specified.
- [x] AC4 — Referential integrity and validation errors (`DuplicateAttackTree`, `UnknownAttackTree`, `AttackTreeInUse`, `InvalidFeasibilityLevel`, `UnknownNodeId`, `GateHasFeasibility`, `DuplicateStepRating`, `NoAttackTree`) are tested in `tests/project.rs` (`threat_attack_tree_integrity_and_validation`, `rejects_every_new_variant`).
- [x] AC5 — `Project::evaluate_tree` and `Project::evaluate_threat` evaluate feasibility with `MaxMin<FeasibilityLevel>` (OR = max, AND/SEQ_AND = min), support `explain` and `why_path`, and reject unrated steps with `MissingLeafValue` and unlinked threats with `NoAttackTree` (`evaluate_threat_feasibility_and_why_path`).
- [x] AC6 — `tests/project.rs` `inverse_round_trip` and `tests/ops_properties.rs` exercise all 20 `Op` variants over both methodologies, checking `check_invariants()` after every step and exact undo round-trip.
- [x] AC7 — `tests/second_methodology.rs` exercises attack trees, feasibility evaluation, and `why_path` over the second methodology with zero library code changes.
- [x] AC8 — `Project::save` and `OpLogWriter::append` refuse attack trees and the 4 new ops with `StoreError::UnencodableOp` before writing any bytes (`save_and_append_refuse_attack_tree_ops_with_unencodable_op`); golden snapshot and op-log fixtures are unchanged.
- [x] AC9 — `errors_never_contain_names` in `tests/project.rs` verifies every new `ModelError` variant's `Display` contains no entity names or node labels.
- [x] AC10 — Neutrality grep in `crates/aegis-model/src` reports only `src/methodology/iso21434.rs` and `mod iso21434;` in `src/methodology/mod.rs`.
- [x] AC11 — No `unwrap()`, `expect()`, `panic!`, `todo!`, or `unimplemented!` in `crates/aegis-model/src` outside `#[cfg(test)]`.
- [x] AC12 — `cargo test --doc` (239 doctests) and `cargo clippy --all-targets -- -D warnings` pass with zero warnings.
- [x] AC13 — `docs/adr/0012-threat-attack-trees.md` written; pointers added in `docs/adr/0008-methodology-neutral-domain-model.md` and `docs/adr/0011-methodology-pack-files.md`.
- [x] AC14 — `scripts/ci.sh crates/aegis-model` and `scripts/deny.sh licenses bans sources` pass.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (156 lib, all integration tests, 239 doctests)
- `cargo test --benches` → pass
- `scripts/ci.sh crates/aegis-model` → pass
- `scripts/deny.sh licenses bans sources` → pass
