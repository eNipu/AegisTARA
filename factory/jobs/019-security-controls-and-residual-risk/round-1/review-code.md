VERDICT: PASS

## Summary

Job `019-security-controls-and-residual-risk` adds `RiskTreatment`, the `SecurityControl` arena (`ControlId`, `ControlIdx`), per-step control mitigations (`StepMitigation`, `MAX_STEP_MITIGATIONS = 16`), the 5 new `Op` variants (`SetRiskTreatment`, `AddSecurityControl`, `RenameSecurityControl`, `RemoveSecurityControl`, `SetStepMitigation`), and residual feasibility/risk evaluation (`residual_step_feasibility`, `evaluate_residual_tree`, `evaluate_residual_threat`, `evaluate_residual_risk`), documented in `docs/adr/0017-security-controls-and-residual-risk.md`.

## Acceptance Criteria Verification

- **AC1 (`RiskTreatment` & `Op::SetRiskTreatment`)**: Verified in `crates/aegis-model/src/threat.rs:14-60`, `crates/aegis-model/src/op.rs:304-316`, and `crates/aegis-model/src/project.rs:2911-2936`. `Op::RemoveThreatScenario` enforces `treatment == None` via `ModelError::ThreatHasRiskTreatment { threat, treatment }` (`src/project.rs:2985-2990`).
- **AC2 (`SecurityControl` arena & ref-counted lifecycle)**: Verified in `crates/aegis-model/src/id.rs:367-377` (`ControlIdx`, `ControlId`), `crates/aegis-model/src/control.rs:1-130` (`SecurityControl`, `StepMitigation`), and `crates/aegis-model/src/project.rs:3013-3078`. `RefCounts::controls` tracks step-mitigation attachments and blocks removal of in-use controls with `ModelError::SecurityControlInUse { control, steps }`.
- **AC3 (`StepMitigation`, `MAX_STEP_MITIGATIONS = 16`, & `Op::SetStepMitigation`)**: Verified in `crates/aegis-model/src/project.rs:105` (`MAX_STEP_MITIGATIONS = 16`), `src/project.rs:1938-1991` (`step_mitigations`, `residual_step_feasibility`), and `src/project.rs:3080-3193` (`set_step_mitigation`). Rejects gate nodes (`ModelError::NotAStep`), enforces `ModelError::TooManyStepMitigations { tree, node, max: 16 }`, maintains `StepMitigation` slices sorted by `ControlIdx`, and blocks removing a tree with active mitigations (`ModelError::AttackTreeHasMitigations { tree, count }`).
- **AC4 (Residual step feasibility & circumvention semantics)**: Verified in `crates/aegis-model/src/project.rs:1968-1991` and `src/project.rs:2053-2189` (`evaluate_residual_tree`, `evaluate_residual_threat`, `evaluate_residual_risk`). Residual feasibility of a step is $\min(\text{inherent}, \min_m m.\text{feasibility})$. On an `Or` gate, mitigating only one branch shifts the gate's value and `why_path` to the unmitigated branch (circumvention); on an `And`/`SeqAnd` gate, mitigating any required step lowers the gate's feasibility to the new bottleneck.
- **AC5 (Snapshot & op-log persistence and decode caps)**: Verified in `crates/aegis-model/src/store/wire.rs:1013-1522`, `crates/aegis-model/src/store/snapshot.rs:70-133`, `crates/aegis-model/src/store/op_log.rs:220-247`, and `crates/aegis-model/tests/store_limits.rs:272-313` (`Limit::StepMitigations { max: 16 }`). Empty outer or inner `step_mitigations` maps in snapshots are rejected at decode time for canonical uniqueness.

## Engineering Standards Audit

- **D1–D5 (Arenas & Typed IDs)**: `ControlIdx(u32)` and `ControlId(Uuid)` use `entity_ids!`; `StepMitigation` is an 8-byte `Copy` struct `(ControlIdx, FeasibilityLevel)`.
- **D7 / P4 (Determinism)**: Snapshot `security_controls` are sorted by `ControlId`, `step_mitigations` outer keys by `NodeId`, and inner keys by `ControlId`.
- **A8 / A9 (Complexity & No $O(n^2)$)**: `set_step_mitigation` and `residual_step_feasibility` are $O(c)$ with $c \le 16$; `evaluate_residual_tree` is $O(V + E)$ (since $c \le 16$ is bounded by a small constant).
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/control.rs`, `src/threat.rs`, `src/project.rs`, and `src/store/`.

## Findings

### Blocking
None.

### Non-Blocking
None.
