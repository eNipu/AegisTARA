# 017-risk-determination: Methodology risk matrices and threat risk determination

Builds on jobs 007 (`docs/adr/0007-methodology-config.md`), 010 (`docs/adr/0010-pack-loader.md`), 011 (`docs/adr/0011-methodology-independence.md`), and 013 (`docs/adr/0012-threat-attack-trees.md`).

## Goal

Complete the ISO/SAE 21434 Clause 15 risk determination chain by defining ordered risk levels and per-category 2D risk matrices (`impact level x feasibility level -> risk level`) in `MethodologyDef` and methodology pack files, and computing per-damage and worst-case risk (`ThreatRisk`, `ImpactRisk`) for any threat scenario with an explain-why trace back to the deciding damage scenario, impact category, and attack-tree path.

## Scope

In (`crates/aegis-model`):
- `RiskLevel(u8)` vocabulary index in `src/methodology/mod.rs` (re-exported at the crate root).
- `risk_levels: Vec<LevelDef>` on `MethodologyDef` and `risk_matrix: Vec<Vec<Arc<str>>>` on `ImpactCategoryDef`.
- Validation in `Methodology::new` (`MethodologyError::RiskLevelCount`, `DuplicateRiskLevel`, `RiskMatrixDimensions`, `UnknownRiskLevel`) and O(1) lookup methods (`risk_levels`, `risk_level_count`, `risk_level`, `risk_level_key`, `risk_level_label`, `risk_for`).
- Pack decode caps in `src/store/mod.rs` and `src/methodology/pack.rs` (`Limit::RiskLevels`, `Limit::RiskMatrixRows`, `Limit::RiskMatrixCols`) and canonical pack writing in `MethodologyDef::write_pack`.
- ISO/SAE 21434:2021 Annex H Table H.8 risk scale (`r1..=r5`) and 4x4 risk matrix in `packs/iso21434-r155.json`, plus medical-device risk scale (`acceptable`, `monitor`, `unacceptable`) in `tests/fixtures/packs/med-device-demo.json`.
- `ImpactRisk` and `ThreatRisk` in `src/threat.rs` (re-exported at the crate root) and `Project::evaluate_risk(ThreatId) -> Result<ThreatRisk, ModelError>` in `src/project.rs`.
- Unit, integration, limit, and property tests, plus ADR 0015 (`docs/adr/0015-risk-determination.md`).

Out:
- Direct threat feasibility without an attack tree (covered by `018-direct-threat-feasibility`).
- Security controls, risk treatment decisions, and residual risk (covered by `019-security-controls-and-residual-risk`).

## Acceptance criteria

- **AC1 (Methodology risk scale & matrix validation):** `Methodology::new` validates `1..=32` unique `risk_levels` and verifies every `ImpactCategoryDef::risk_matrix` has `levels.len()` rows by `feasibility_levels.len()` columns of valid risk-level keys, rejecting malformed definitions with typed `MethodologyError` variants carrying no user strings.
- **AC2 (Pack round-trip & decode caps):** `MethodologyDef::read_pack` enforces `Limit::RiskLevels`, `Limit::RiskMatrixRows`, and `Limit::RiskMatrixCols` at decode time before `Methodology::new` runs, and `write_pack` produces canonical JSON.
- **AC3 (Built-in ISO 21434 Table H.8 & second methodology):** `Methodology::iso21434_r155()` exposes `r1..=r5` and maps `(ImpactCategory, Level, FeasibilityLevel)` according to ISO/SAE 21434 Table H.8; `med-device-demo.json` uses a distinct 3-level risk scale and non-square (3x3 and 4x3) matrices without code changes.
- **AC4 (Threat risk determination & explain-why):** `Project::evaluate_risk` computes `ImpactRisk` entries for every `(damage, category)` pair on a threat's linked damage scenarios, identifies `max_risk()` and `deciding_entry()`, and preserves `tree_evaluation()` so callers can trace both the deciding damage impact and the decisive attack-tree steps.
