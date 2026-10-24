# Build Report: 026-choke-points-and-sensitivity

## Summary

Implemented attack-path choke-point analysis (`ChokePoint<V>`), greedy minimum hitting-set computation (`top_k_hitting_set`), and $\pm 1$ single-parameter tipping-point sensitivity analysis (`StepTippingPoint`, `FeasibilityTippingPoint`, `ImpactTippingPoint`, `ThreatSensitivity`) across `Evaluation` and `Project` inherent/residual evaluations in `crates/aegis-model`.

## Files Changed

- `docs/adr/0024-choke-points-and-sensitivity.md` — Recorded the architectural design for choke points, greedy hitting sets, incremental tree tipping-point evaluation via `TreeEvalCache`, and $O(1)$-per-entry impact tipping-point calculation.
- `crates/aegis-model/src/methodology/mod.rs` — Made `Level::new(u8)` `pub(crate)` and added `pub(crate) fn new(i: u8) -> Self` on `FeasibilityLevel` for crate-internal $\pm 1$ scale index construction.
- `crates/aegis-model/src/paths.rs` — Added `ChokePoint<V>` with accessors (`step`, `path_count`, `bottleneck_count`, `easiest_path_feasibility`), doctests, and unit tests.
- `crates/aegis-model/src/eval.rs` — Added `Evaluation::choke_points` and `Evaluation::top_k_hitting_set` with complexity annotations and doctests.
- `crates/aegis-model/src/sensitivity.rs` — Created module defining `StepTippingPoint`, `FeasibilityTippingPoint`, `ImpactTippingPoint`, and `ThreatSensitivity` with full rustdoc and compiling doctests.
- `crates/aegis-model/src/project.rs` — Added `tree_choke_points`, `residual_tree_choke_points`, `threat_choke_points`, `residual_threat_choke_points`, `tree_hitting_set`, `residual_tree_hitting_set`, `threat_hitting_set`, `residual_threat_hitting_set`, `tree_tipping_points`, `residual_tree_tipping_points`, `threat_sensitivity`, and `residual_threat_sensitivity`.
- `crates/aegis-model/src/lib.rs` — Declared `mod sensitivity;` and re-exported `ChokePoint`, `FeasibilityTippingPoint`, `ImpactTippingPoint`, `StepTippingPoint`, and `ThreatSensitivity`.
- `crates/aegis-model/tests/eval_properties.rs` — Extended random DAG property tests to verify `choke_points` and `top_k_hitting_set` invariants and oracle agreement.
- `crates/aegis-model/tests/ops_properties.rs` — Added property tests `sensitivity_and_choke_points_first_methodology` and `sensitivity_and_choke_points_second_methodology` verifying tipping-point sensitivity against a full-clone perturbation oracle.
- `crates/aegis-model/tests/project.rs` — Added `choke_points_and_hitting_set_on_project` and `sensitivity_and_tipping_points_on_project` covering inherent/residual choke points, hitting sets, step/direct feasibility tipping points, tied vs unique impact tipping points, error cases, and rule R6 (`SENTINEL` name-leak checks).

## Verification

- `cargo fmt --manifest-path crates/aegis-model/Cargo.toml` — passed.
- `./scripts/ci.sh` — passed (`fmt`, `clippy --all-targets -- -D warnings`, all unit, integration, property, and doc tests).
- `./scripts/deny.sh` — passed (`advisories`, `bans`, `licenses`, `sources`).
