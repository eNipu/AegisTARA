VERDICT: PASS

Surface reviewed: methodology risk-matrix API (`RiskLevel`, `Methodology::risk_levels`, `Methodology::risk_for`), threat risk evaluation (`Project::evaluate_risk`, `ThreatRisk`, `ImpactRisk`), and `MethodologyError` / `ModelError` / `PackError` diagnostics. No UI, CLI, or server surface in this change.

Flow:
- `Methodology::risk_for(category, impact, feasibility)` performs an $O(1)$ lookup into a flat row-major matrix (`impact.index() * n_feasibility + feasibility.index()`) and returns `Option<RiskLevel>`.
- `Project::evaluate_risk(threat_id)` evaluates a threat scenario in a single call and returns `ThreatRisk`, exposing:
  - `feasibility()` and `tree_evaluation()` (`Option<&Evaluation<FeasibilityLevel>>`) for step-by-step attack-tree explainability (`why_path()`).
  - `entries()` (`&[ImpactRisk]` in canonical ascending `(DamageId, ImpactCategory)` order), each carrying `damage()`, `category()`, `impact()`, and `risk()`.
  - `max_risk()` and `deciding_entry()` (`Option<&ImpactRisk>`, with deterministic first-entry tie-breaking) so an automotive security engineer can immediately see both the worst-case risk level and which `(DamageId, ImpactCategory, Level)` tuple and attack-tree path produced it.
- Evaluation runs in microseconds per threat, well within the 50 ms interactive budget.

Errors:
- `MethodologyError::RiskLevelCount`, `DuplicateRiskLevel`, `RiskMatrixDimensions { category, expected_rows, expected_cols, found_rows, found_cols }`, and `UnknownRiskLevel { category, impact, feasibility, key }` pinpoint the exact misconfigured category and cell coordinates while truncating untrusted keys to at most 64 bytes.
- `PackError::CollectionTooLarge { limit: Limit::Levels, max: 16 }` enforces scale bounds during YAML deserialization.
- `ModelError::ThreatMissingFeasibility`, `ModelError::DamageMissingImpact`, and `ModelError::ThreatNoEvaluatedRisks` clearly state which `ThreatId` or `DamageId` is incomplete without leaking confidential scenario names.
