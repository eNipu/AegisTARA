# Build: `017-risk-determination`

## Changes
- `crates/aegis-model/src/methodology/mod.rs`, `src/methodology/pack.rs`, `src/store/mod.rs`, `src/error.rs`:
  - Added `RiskLevel` vocabulary index, `MethodologyDef::risk_levels`, and `ImpactCategoryDef::risk_matrix`.
  - Added `MethodologyError::{RiskLevelCount, DuplicateRiskLevel, RiskMatrixDimensions, UnknownRiskLevel}`, `ModelError::UnknownRiskLevel`, and `Limit::{RiskLevels, RiskMatrixRows, RiskMatrixCols}`.
  - Added `Methodology::{risk_levels, risk_level_count, risk_level, risk_level_key, risk_level_label, risk_for}`.
- `crates/aegis-model/packs/iso21434-r155.json` & `tests/fixtures/packs/med-device-demo.json`:
  - Added ISO/SAE 21434:2021 Table H.8 risk scale (`r1..=r5`) and 4x4 matrices to `iso21434-r155.json`, and medical-device risk scale (`acceptable`, `monitor`, `unacceptable`) and matrices to `med-device-demo.json`.
- `crates/aegis-model/src/threat.rs`, `src/project.rs`, `src/lib.rs`:
  - Added `ImpactRisk`, `ThreatRisk`, and `Project::evaluate_risk(ThreatId) -> Result<ThreatRisk, ModelError>`, re-exporting `RiskLevel`, `ImpactRisk`, and `ThreatRisk` at the crate root.
- `crates/aegis-model/tests/*.rs`, `docs/adr/0015-risk-determination.md`:
  - Added unit, integration, limit, and property tests for risk levels, matrices, and `evaluate_risk`, and recorded ADR 0015.

## Verification
- `cargo test --manifest-path crates/aegis-model/Cargo.toml`
- `cargo test --manifest-path crates/aegis-model/Cargo.toml --doc`
