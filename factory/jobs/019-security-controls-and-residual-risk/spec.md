# 019 — Security controls and residual risk in `aegis-model`

## Goal

Add `SecurityControl` entities, `RiskTreatment` decisions on threat scenarios, control attachments that mitigate attack steps (`StepMitigation`), and dual inherent vs. residual attack-tree and risk evaluation so analysts can verify whether security controls reduce threat risk or are circumvented by alternative `Or` branches.

## Scope

1. **IDs and domain types (`id.rs`, `control.rs`, `threat.rs`)**:
   - `ControlIdx` and `ControlId` via `entity_ids!`.
   - `MAX_STEP_MITIGATIONS: usize = 16`.
   - `SecurityControl` (`id`, `name`, `provenance`).
   - `StepMitigation` (`control: ControlIdx`, `feasibility: FeasibilityLevel`).
   - `RiskTreatment` (`Avoid`, `Reduce`, `Share`, `Retain`) and `ThreatScenario::treatment()`.
2. **Errors and caps (`error.rs`, `store/mod.rs`)**:
   - `EntityKind::SecurityControl`.
   - `ModelError` variants: `DuplicateSecurityControl`, `UnknownSecurityControl`, `AttackTreeHasMitigations`, `SecurityControlInUse`, `ThreatHasRiskTreatment`, `TooManyStepMitigations`.
   - `Limit::StepMitigations { max: usize }` (`Limit::STEP_MITIGATIONS`).
3. **Operations and `Project` state (`op.rs`, `project.rs`, `lib.rs`)**:
   - `Op::SetRiskTreatment`, `Op::AddSecurityControl`, `Op::RenameSecurityControl`, `Op::RemoveSecurityControl`, `Op::SetStepMitigation`.
   - `Project` security control accessors, `step_mitigations`, `residual_step_feasibility`, `evaluate_residual_tree`, `evaluate_residual_threat`, and `evaluate_residual_risk`.
4. **Persistence (`store/{wire,snapshot,op_log}.rs`)**:
   - Wire encoding/decoding with decode-time caps for `security_controls`, `treatment`, `step_mitigations`, and all 5 new `Op` variants while preserving v1 fixture compatibility.
5. **Tests and ADR**:
   - Unit, integration, property, and store limit tests, plus `docs/adr/0017-security-controls-and-residual-risk.md`.
