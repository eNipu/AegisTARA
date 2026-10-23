# Spec: `022-project-diff`

## Goal

Implement deterministic project version diff and change impact analysis (`Project::diff`) in `crates/aegis-model` (rule A7).

## Requirements

1. **Methodology Compatibility:**
   - Add `ModelError::MethodologyMismatch { expected, expected_version, found, found_version }` when comparing two projects with unequal methodologies.
2. **Diff Data Model (`src/diff.rs`):**
   - `EntityDiff<Id>`: `added() -> &[Id]`, `removed() -> &[Id]`, `modified() -> &[Id]`, `is_empty() -> bool`, `len() -> usize`.
   - `RiskSnapshot`: `feasibility() -> FeasibilityLevel`, `max_risk() -> Option<RiskLevel>`.
   - `RiskChange`: `Unchanged`, `Increased`, `Decreased`, `NewlyEvaluated`, `NoLongerEvaluated` with `as_str` and `Display`.
   - `RiskDelta`: `before() -> Option<RiskSnapshot>`, `after() -> Option<RiskSnapshot>`, `change() -> RiskChange`, `is_changed() -> bool`.
   - `ThreatRiskDelta`: `threat() -> ThreatId`, `inherent() -> RiskDelta`, `residual() -> RiskDelta`, `is_risk_changed() -> bool`.
   - `ProjectDiff`: `assets()`, `damage_scenarios()`, `threat_scenarios()`, `attack_trees()`, `security_controls()`, `risk_deltas()`, `changed_risks()`, `is_empty()`.
   - Zero entity names, labels, or actor strings stored in any diff type (rule R6).
3. **Evaluation & Impact Propagation (`Project::diff`):**
   - Sorted merge-join across all 5 entity arenas in canonical ascending ID order (rule P4).
   - Change impact propagation from modified assets, damage scenarios, attack trees, and security controls to referencing threat scenarios.
   - `p1.diff(&p2)?.is_empty() == (p1 == p2)` and risk delta completeness verified by property tests over both methodologies.
