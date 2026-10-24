VERDICT: PASS

Surface reviewed: security controls and residual-risk API (`ControlId`, `ControlIdx`, `SecurityControl`, `StepMitigation`, `RiskTreatment`, 5 new `Op` variants, `Project::residual_step_feasibility`, `Project::evaluate_residual_tree`, `Project::evaluate_residual_threat`, `Project::evaluate_residual_risk`), store persistence, undo/redo invariants, and `ModelError` / `StoreError` diagnostics. No UI, CLI, or server surface in this change.

Flow:
- Analysts record risk treatment decisions (`RiskTreatment::{Avoid, Reduce, Share, Retain}`) via `Op::SetRiskTreatment`, manage reusable project-scoped `SecurityControl` entities (`AddSecurityControl`, `RenameSecurityControl`, `RemoveSecurityControl`), and attach mitigations to attack-tree leaf steps via `Op::SetStepMitigation { tree, step, mitigation, provenance }`.
- Inherent vs. residual comparison is symmetric and effortless: `evaluate_tree` / `evaluate_residual_tree`, `evaluate_threat` / `evaluate_residual_threat`, and `evaluate_risk` / `evaluate_residual_risk` return the same `Evaluation<FeasibilityLevel>` and `ThreatRisk` types. When a mitigated leaf is bypassed by an unmitigated sibling under an `Or` gate, `evaluate_residual_risk` surfaces the unmitigated sibling in `Evaluation::why_path()` so the analyst immediately sees why residual risk did not drop.
- All 5 new `Op` variants return exact single-step inverses that restore prior values and `Provenance` in $O(1)$ (or $O(\log C)$), and `RemoveAttackTree`, `RemoveSecurityControl`, and `RemoveThreatScenario` enforce pre-removal cleanup so undo never requires multi-op batches.

Errors:
- `ModelError::ControlNotFound`, `DuplicateControl`, `SecurityControlInUse { control, tree_count }`, `AttackTreeHasMitigations { tree, count }`, `ThreatHasRiskTreatment { threat, treatment }`, and `TooManyStepMitigations { tree, max }` clearly report blocking entity IDs, counts, and limits without exposing confidential control names or tree labels.
- `StoreError::CollectionTooLarge { limit: Limit::SecurityControls | Limit::StepMitigations, max }` enforces wire decode bounds before model validation.
