# 018 — Build Log

## Summary

- Added `ModelError::ThreatHasDirectFeasibility { threat: ThreatId, feasibility: FeasibilityLevel }` in `crates/aegis-model/src/error.rs`.
- Added `direct_feasibility: Option<FeasibilityLevel>` and `ThreatScenario::direct_feasibility` (with runnable doctest) in `crates/aegis-model/src/threat.rs`.
- Added `Op::SetThreatFeasibility` in `crates/aegis-model/src/op.rs`.
- Updated `Project::validate`, `Project::commit`, `Project::check_remove_threat`, `Project::same_threat`, `Project::check_invariants`, and `Project::evaluate_risk` in `crates/aegis-model/src/project.rs`.
- Added snapshot (`ThreatView::direct_feasibility`, `ThreatWire::direct_feasibility`) and op-log (`OpView::SetThreatFeasibility`, `OpWire::SetThreatFeasibility`) persistence in `crates/aegis-model/src/store/{snapshot,op_log}.rs`.
- Extended `crates/aegis-model/tests/common/mod.rs`, `tests/project.rs`, `tests/ops_properties.rs`, and `tests/store.rs` to cover `Op::SetThreatFeasibility`, `ModelError::ThreatHasDirectFeasibility`, direct-feasibility risk evaluation, and attack-tree override/fallback.
- Recorded `docs/adr/0016-direct-threat-feasibility.md`.
