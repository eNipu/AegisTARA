VERDICT: PASS

## Summary

Job `022-project-diff` implements deterministic project version diff and change impact analysis (`EntityDiff<Id>`, `RiskChange`, `RiskSnapshot`, `RiskDelta`, `ThreatRiskDelta`, `ProjectDiff`, `Project::diff`, and `ModelError::MethodologyMismatch`) in `crates/aegis-model/src/diff.rs` and `crates/aegis-model/src/project.rs:2530-2728`, documented in `docs/adr/0020-project-diff.md`.

## Acceptance Criteria Verification

- **AC1 (Methodology compatibility check)**: Verified in `crates/aegis-model/src/project.rs:2560-2567` and `crates/aegis-model/src/error.rs:65-75`. `Project::diff` checks `Arc::ptr_eq(&self.methodology, &other.methodology) || *self.methodology == *other.methodology` and returns `Err(ModelError::MethodologyMismatch { expected, expected_version, found, found_version })` when methodologies differ.
- **AC2 (Sorted merge-join entity diffs across all 5 arenas)**: Verified in `crates/aegis-model/src/diff.rs:84-137` (`diff_arena`) and `crates/aegis-model/src/project.rs:2569-2658` (`same_threat`, `same_tree`). Because `Arena::iter` yields entities in ascending `UUIDv7` order, `diff_arena` performs a single two-pointer merge-join in $O(N_{\text{base}} + N_{\text{curr}})$ time, comparing cross-arena references by public IDs (`AssetId`, `DamageId`, `TreeId`, `ControlId`) so slot index differences never cause false positives.
- **AC3 (Direct & indirect change impact propagation to `ThreatRiskDelta`)**: Verified in `crates/aegis-model/src/project.rs:2601-2656`. A threat scenario is included in `risk_deltas()` whenever it is added, removed, or directly modified, or when its target `AssetId`, any linked `DamageId`, its linked `TreeId`, or any `ControlId` mitigating a step of its linked attack tree is in the respective `modified()` slice (checked via `binary_search`).
- **AC4 (`RiskSnapshot`, `RiskDelta`, `RiskChange`, and `changed_risks()`)**: Verified in `crates/aegis-model/src/diff.rs:139-447`. Both inherent (`evaluate_risk`) and residual (`evaluate_residual_risk`) risk snapshots are compared, classifying directional changes (`Unchanged`, `Increased`, `Decreased`, `NewlyEvaluated`, `NoLongerEvaluated`). Property-tested in `crates/aegis-model/tests/ops_properties.rs:60-191` (`diff_soundness_first_methodology`, `diff_soundness_second_methodology`), proving symmetry, reflexivity, `fwd.is_empty() == (base == curr)`, and completeness of `changed_risks()` across all threats.
- **AC5 (No sensitive names/labels/actors in diff or error types, R6)**: Verified in `src/diff.rs` and `src/error.rs`. `ProjectDiff` and `ModelError::MethodologyMismatch` hold only `UUIDv7` IDs, 1-byte vocabulary indices, and methodology keys/versions.

## Engineering Standards Audit

- **A7 (Version Diff & Change Impact Analysis)**: Deterministic entity-level and risk-level delta analysis across project versions.
- **D1–D5 (Arenas & Typed IDs)**: `RiskSnapshot` (3 bytes), `RiskDelta` (6 bytes), and `ThreatRiskDelta` (28 bytes) are `Copy` value types over typed IDs and 1-byte indices.
- **D7 / P4 (Determinism)**: All `added`, `removed`, `modified`, and `risk_deltas` slices are strictly sorted by `UUIDv7` and independent of op-application history.
- **A8 / A9 (Complexity & No $O(n^2)$)**: Arena merge-joins run in $O(N_{\text{base}} + N_{\text{curr}})$, membership in `modified()` slices uses $O(\log N)$ binary search, and risk evaluation runs only for affected threats.
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/diff.rs`, `src/project.rs`, and `src/error.rs`.

## Findings

### Blocking
None.

### Non-Blocking
None.
