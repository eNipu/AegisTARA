# Build Log — 019-security-controls-and-residual-risk

## Summary of Changes

1. **IDs and Control Module (`crates/aegis-model/src/id.rs`, `crates/aegis-model/src/control.rs`)**:
   - Added `ControlIdx` and `ControlId` in `id.rs`.
   - Created `control.rs` with `MAX_STEP_MITIGATIONS = 16`, `SecurityControl`, `StepMitigation`, and `RiskTreatment`.
2. **Errors and Store Limits (`crates/aegis-model/src/error.rs`, `crates/aegis-model/src/store/mod.rs`)**:
   - Added `EntityKind::SecurityControl` and `ModelError` variants `DuplicateSecurityControl`, `UnknownSecurityControl`, `AttackTreeHasMitigations`, `SecurityControlInUse`, `ThreatHasRiskTreatment`, and `TooManyStepMitigations`.
   - Added `Limit::StepMitigations { max: 16 }`.
3. **Operations, `Project`, and Residual Evaluation (`crates/aegis-model/src/{threat,op,project,lib}.rs`)**:
   - Added `ThreatScenario::treatment` and 5 new `Op` variants (`SetRiskTreatment`, `AddSecurityControl`, `RenameSecurityControl`, `RemoveSecurityControl`, `SetStepMitigation`).
   - Added `SecurityControl` arena and reference counting (`RefCounts::controls`), per-step `step_mitigations` on `TreeEntry`, `residual_step_feasibility`, `evaluate_residual_tree`, `evaluate_residual_threat`, and `evaluate_residual_risk`.
4. **Store Persistence (`crates/aegis-model/src/store/{wire,snapshot,op_log}.rs`)**:
   - Added snapshot and op-log serialization and capped deserialization for `security_controls`, `treatment`, `step_mitigations`, and the 5 new `Op` variants.
5. **Tests and Documentation**:
   - Updated `tests/common/mod.rs`, `tests/ops_properties.rs`, `tests/project.rs`, `tests/store.rs`, and `tests/store_limits.rs`.
   - Added `docs/adr/0017-security-controls-and-residual-risk.md`.
