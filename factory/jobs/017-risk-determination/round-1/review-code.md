VERDICT: PASS

## Summary

Job `017-risk-determination` adds methodology-defined risk scales (`RiskLevel`) and per-impact-category 2D risk lookup matrices (`ImpactCategoryDef::risk_matrix`) to `MethodologyDef` and `Methodology`, updates the built-in `packs/iso21434-r155.json` pack with ISO/SAE 21434 Table H.8 (`r1`..`r5`), and implements `ImpactRisk`, `ThreatRisk`, and `Project::evaluate_risk` with full explain-why traceability (`deciding_entry` and `tree_evaluation`), documented in `docs/adr/0015-risk-determination.md`.

## Acceptance Criteria Verification

- **AC1 (`RiskLevel` & methodology risk matrix validation)**: Verified in `crates/aegis-model/src/methodology/mod.rs:170-189` (`RiskLevel(u8)`), `src/methodology/mod.rs:344-354` (`ImpactCategoryDef::risk_matrix`), `src/methodology/mod.rs:394-403` (`MethodologyDef::risk_levels`), and `src/methodology/mod.rs:1395-1452` (`validate_risk_levels`, `compile_risk_matrix`). Validates `RiskLevelCount`, `DuplicateRiskLevel`, `RiskMatrixDimensions`, and `UnknownRiskLevel`.
- **AC2 (`Methodology::risk_for` $O(1)$ lookup & accessors)**: Verified in `crates/aegis-model/src/methodology/mod.rs:1012-1102`. Each category compiles its matrix into a flat row-major `Box<[RiskLevel]>` (`row * feasibility_levels.len() + col`), giving $O(1)$ time and space lookup in `risk_for`.
- **AC3 (Pack wire encoding & decode-time caps)**: Verified in `crates/aegis-model/src/methodology/pack.rs:286-380`, `packs/iso21434-r155.json`, `tests/fixtures/packs/med-device-demo.json`, `tests/builtin_pack.rs:197-222`, and `tests/pack_limits.rs`.
- **AC4 (`ImpactRisk`, `ThreatRisk`, and `Project::evaluate_risk`)**: Verified in `crates/aegis-model/src/threat.rs:189-378` and `crates/aegis-model/src/project.rs:2125-2142`. `ThreatRisk` exposes `feasibility()`, `tree_evaluation()`, `entries()`, `max_risk()`, and `deciding_entry()` (first entry attaining `max_risk()` in damage-link order then ascending `ImpactCategory` order).
- **AC5 (Derived data not persisted, D10)**: Verified that `ThreatRisk` and `ImpactRisk` are computed on demand and never stored in `Project` arenas, snapshots, or op logs.

## Engineering Standards Audit

- **P1 (Core vs. Methodology Pack Boundary)**: Risk levels (`r1`..`r5` in ISO 21434 vs. `acceptable`/`monitor`/`unacceptable` in `med-device-demo`) and 2D risk matrices live entirely in methodology packs.
- **D1–D5 (Arenas & Typed IDs)**: `RiskLevel(u8)` is a 1-byte vocabulary index; `ImpactRisk` stores `(DamageId, ImpactCategory, Level, FeasibilityLevel, RiskLevel)` (`Copy`, 20 bytes).
- **D7 / P4 (Determinism)**: `ThreatRisk::entries()` iterates `threat.damages()` in canonical `DamageId` order and `damage.impacts()` in ascending `ImpactCategory` order; ties in `max_risk` are broken deterministically by the first attaining `(DamageId, ImpactCategory)` pair.
- **A8 / A9 (Complexity & No $O(n^2)$)**: `Methodology::risk_for` is $O(1)$; `Project::evaluate_risk` is $O(V + E + d \cdot c)$ time and $O(V + d \cdot c)$ space.
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/methodology/`, `src/threat.rs`, and `src/project.rs`.

## Findings

### Blocking
None.

### Non-Blocking
- **Observation 1 (`src/methodology/mod.rs:1395-1400` vs. `docs/adr/0015-risk-determination.md`)**: `validate_risk_levels` enforces `MIN_LEVELS..=MAX_LEVELS` (`2..=16`) and `Limit::LEVELS` (`max: 16`) for `risk_levels` and `risk_matrix` rows/columns, matching `feasibility_levels` and `ImpactCategoryDef::levels` (and documented in `build.md`), whereas `docs/adr/0015-risk-determination.md` still mentions `1..=32` / `Limit::RiskLevels { max: 32 }`. The `2..=16` bound is cleaner and consistent with all other level scales in `methodology`.
- **Observation 2 (`src/methodology/mod.rs:1760`)**: `#[allow(dead_code)] pub(crate) fn check_risk_level(&self, level: RiskLevel) -> Result<(), ModelError>` and `ModelError::UnknownRiskLevel` are defined and unit-tested (`src/methodology/mod.rs:1975-1980`) as a symmetry helper alongside `check_feasibility_level`, though no `Op` takes a `RiskLevel` directly because risk levels are derived via `risk_for`.
