VERDICT: PASS

## Summary

Job `020-project-lint` implements the deterministic project completeness and consistency linter (`LintSeverity`, `LintCode`, `LintFinding`, `Project::lint`) in `crates/aegis-model/src/lint.rs` and `crates/aegis-model/src/project.rs:2191-2352`, documented in `docs/adr/0018-project-lint.md`.

## Acceptance Criteria Verification

- **AC1 (13 `LintCode` variants across `Warning` and `Error` severities)**: Verified in `crates/aegis-model/src/lint.rs:19-280`:
  - Warnings (9): `AssetWithoutThreats`, `AssetPropertyUntested` (emitted only when the asset has at least one threat so it never duplicates `AssetWithoutThreats`), `DamageScenarioWithoutImpacts`, `DamageScenarioUnlinked`, `ThreatWithoutDamages`, `RedundantDirectFeasibility`, `ThreatWithoutTreatment`, `AttackTreeUnlinked`, `SecurityControlUnused`.
  - Errors (4): `ThreatWithoutFeasibility`, `ReduceTreatmentWithoutControls`, `AttackTreeGateEmpty`, `AttackTreeStepUnrated`.
- **AC2 (Single-pass `RefCounts`-backed `Project::lint` in linear time)**: Verified in `crates/aegis-model/src/project.rs:2236-2352`. `Project::lint` reads `self.refs.assets`, `self.refs.asset_properties`, `self.refs.damages`, `self.refs.trees`, and `self.refs.controls` in $O(1)$ / $O(\log P)$ per entity without rescanning the threat arena or allocating temporary hash maps.
- **AC3 (Soundness invariant: zero `Error` findings guarantees error-free evaluation)**: Verified in `crates/aegis-model/src/project.rs:2206-2214` and property-tested in `crates/aegis-model/tests/ops_properties.rs:14-35` (`lint_soundness_first_methodology`, `lint_soundness_second_methodology`). When `findings.iter().all(|f| !f.is_error())`, `evaluate_tree`, `evaluate_residual_tree`, `evaluate_threat`, `evaluate_residual_threat`, `evaluate_risk`, and `evaluate_residual_risk` all return `Ok`.
- **AC4 (Determinism & history independence, P4)**: Verified in `src/project.rs:2350` (`findings.sort_unstable()`) and unit/integration tests (`lint_is_history_independent_and_never_leaks_names`).
- **AC5 (No sensitive names/labels/actors in `LintFinding` or `Display`/`Debug`, R6)**: Verified in `crates/aegis-model/src/lint.rs:64-322` (`size_of::<LintCode>() <= 40`, all fields are IDs or `Property`).

## Engineering Standards Audit

- **D1–D5 (Arenas & Typed IDs)**: `LintCode` is `Copy` (`<= 40` bytes, enforced by compile-time assertion at `src/lint.rs:230`).
- **D7 / P4 (Determinism)**: Findings are sorted by `(LintSeverity, LintCode)` and independent of insertion/deletion history.
- **A8 / A9 (Complexity & No $O(n^2)$)**: Documented and implemented in $O(A P + D + T + K V + C + F \log F)$ time and $O(F)$ space.
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/lint.rs` and `src/project.rs`.

## Findings

### Blocking
None.

### Non-Blocking
None.
