# 018 — Direct threat feasibility

## Problem

During concept-phase TARA (ISO/SAE 21434 Clause 15.6.2), analysts often rate a threat scenario's attack feasibility directly without constructing a multi-step attack tree, and later refine high-risk threats with an attack tree. `aegis-model` previously required every threat scenario to link an `AttackTree` before `Project::evaluate_risk` could compute risk.

## Acceptance Criteria

- **AC1**: `ThreatScenario` stores an optional direct feasibility rating (`ThreatScenario::direct_feasibility(&self) -> Option<FeasibilityLevel>`).
- **AC2**: `Op::SetThreatFeasibility { threat, level, provenance }` sets or clears a threat scenario's direct feasibility rating, validates `level` against the project's methodology, updates the threat's provenance, and returns the prior `Op::SetThreatFeasibility` as its exact inverse.
- **AC3**: `Op::RemoveThreatScenario` rejects removing a threat while `direct_feasibility` is `Some` with `ModelError::ThreatHasDirectFeasibility { threat, feasibility }` so `Op::AddThreatScenario` remains the exact single-op inverse.
- **AC4**: `Project::evaluate_risk` uses the linked attack tree's evaluated root feasibility when `threat.attack_tree()` is `Some` (`tree_evaluation: Some(...)`), falls back to `threat.direct_feasibility()` when `threat.attack_tree()` is `None` (`tree_evaluation: None`), and returns `Err(ModelError::NoAttackTree(id))` only when both are `None`.
- **AC5**: Snapshot (`aegis-project`) and op-log (`aegis-oplog`) persist `direct_feasibility` and `Op::SetThreatFeasibility` using methodology keys while omitting `direct_feasibility` when `None` so v1 golden fixtures remain byte-for-byte identical.
- **AC6**: Recorded in `docs/adr/0016-direct-threat-feasibility.md`.
