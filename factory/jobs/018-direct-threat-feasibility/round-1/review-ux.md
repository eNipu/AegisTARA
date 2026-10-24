VERDICT: PASS

Surface reviewed: direct threat feasibility API (`Op::SetThreatFeasibility`, `ThreatScenario::direct_feasibility`, `Project::evaluate_threat`, `Project::evaluate_risk`), store persistence, undo/redo invariants, and `ModelError` diagnostics. No UI, CLI, or server surface in this change.

Flow:
- Concept-phase TARA analysts can assign or clear a direct feasibility rating on a threat scenario in one step via `Op::SetThreatFeasibility { threat, level, provenance }` and inspect it via `ThreatScenario::direct_feasibility()`.
- Precedence is intuitive and non-destructive: when a threat has no linked attack tree, `Project::evaluate_threat` and `Project::evaluate_risk` use `direct_feasibility` (returning `ThreatRisk::tree_evaluation() == None`). When an attack tree is later linked via `Op::SetThreatAttackTree`, tree evaluation takes precedence (`tree_evaluation() == Some(...)`) while preserving `direct_feasibility` on the threat so unlinking the tree seamlessly falls back to the concept-phase rating.
- `Op::SetThreatFeasibility` returns an exact single-step inverse restoring the previous `Option<FeasibilityLevel>` and `Provenance`.

Errors:
- `Op::RemoveThreatScenario` refuses to remove a threat while `direct_feasibility.is_some()`, returning `ModelError::ThreatHasDirectFeasibility { threat, feasibility }` (`"threat scenario {threat} still has direct feasibility {feasibility:?} assigned"`), preserving the single-step `Op::AddThreatScenario` undo invariant.
- `ModelError::ThreatMissingFeasibility(threat)` clearly states `"threat scenario {0} has neither a linked attack tree nor a direct feasibility rating"`.
