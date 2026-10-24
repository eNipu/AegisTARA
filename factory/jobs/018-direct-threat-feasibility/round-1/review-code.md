VERDICT: PASS

## Summary

Job `018-direct-threat-feasibility` adds optional direct threat feasibility ratings (`ThreatScenario::direct_feasibility`, `Op::SetThreatFeasibility`, `ModelError::ThreatHasDirectFeasibility`), attack-tree override precedence in `Project::evaluate_risk`, and backward-compatible snapshot/op-log persistence, documented in `docs/adr/0016-direct-threat-feasibility.md`.

## Acceptance Criteria Verification

- **AC1 (`ThreatScenario::direct_feasibility` & `Op::SetThreatFeasibility`)**: Verified in `crates/aegis-model/src/threat.rs:51-130`, `crates/aegis-model/src/op.rs:289-302`, and `crates/aegis-model/src/project.rs:2882-2909`. Validates `level` via `check_feasibility_level`, updates provenance, and returns the exact prior `Op::SetThreatFeasibility` inverse in $O(1)$ time and $O(1)$ space.
- **AC2 (Attack-tree override precedence in `Project::evaluate_risk`)**: Verified in `crates/aegis-model/src/project.rs:2125-2189` (`evaluate_risk_with`). When `threat.attack_tree()` is `Some(tree_idx)`, the linked attack tree's root feasibility is used and `ThreatRisk::tree_evaluation()` is `Some(&eval)`. When `threat.attack_tree()` is `None`, `threat.direct_feasibility()` is used and `tree_evaluation()` is `None`. When both are `None`, `Err(ModelError::NoAttackTree(id))` is returned. Unlinking the attack tree restores the stored `direct_feasibility`.
- **AC3 (Referential integrity on `Op::RemoveThreatScenario`)**: Verified in `crates/aegis-model/src/project.rs:2979-2984`. Removing a threat while `direct_feasibility` is `Some(feasibility)` fails with `ModelError::ThreatHasDirectFeasibility { threat: id, feasibility }`, keeping `Op::AddThreatScenario` as the exact single-op inverse.
- **AC4 (Snapshot & op-log persistence)**: Verified in `crates/aegis-model/src/store/wire.rs:845-918`, `crates/aegis-model/src/store/snapshot.rs:416-430`, and `crates/aegis-model/src/store/op_log.rs:211-215`. `direct_feasibility` is omitted on snapshots when `None` (preserving v1 golden fixtures) and rejects explicit `null` via `deserialize_with = "some"`.

## Engineering Standards Audit

- **D1–D5 (Arenas & Typed IDs)**: `direct_feasibility: Option<FeasibilityLevel>` adds 1 byte to `ThreatScenario`.
- **D7 / P4 (Determinism)**: `Project::load` replays `AddThreatScenario` followed by `SetThreatFeasibility` with the threat's snapshot provenance, yielding identical `Project` state and canonical bytes on `save`.
- **A8 / A9 (Complexity & No $O(n^2)$)**: `Op::SetThreatFeasibility` is $O(1)$ time and space; direct-feasibility `evaluate_risk` is $O(d \cdot c)$ time and space.
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/threat.rs`, `src/op.rs`, `src/project.rs`, and `src/store/`.

## Findings

### Blocking
None.

### Non-Blocking
None.
