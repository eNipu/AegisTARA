VERDICT: PASS
## For the owner
What changed: `Project::applicability()` and `ProjectEvalCache` now exclude rated attack steps outside the attacker profile (`Reason::OutsideProfile`) and explicitly scaled threats filtered by scalability (`Reason::Scalability`), while skipping factor/scaling applicability work under the default filter. Added 100,000-node applicability benchmarks plus `aegis-client` edits, undo labels, exclusion strings, toast text, Fluent translations, and headless `AttackerProfileView`.
Look at:
- `crates/aegis-model/src/applicability.rs:620` — `Project::applicability()` fast-path `is_default()` guard, `Reason::Scalability` before `propagate_owners`, and lazy `excluded` / `step_factors` check in `tree_exclusions` (`:766`).
- `crates/aegis-model/src/project.rs:13760` — `may_change_applicability(op, filter)` (`const fn` gating `SetThreatScaling` and `SetStepFactor` rechecks) and `ProjectEvalCache::update` (`:14174`).
- `crates/aegis-client/src/attacker_profile.rs:103` — `attacker_profile_view` (columns, chips, scalability options, excluded counts, localized labels/explainer) and edit constructors (`:197-225`).
Risk: When `Op::SetStepFactor` flips a tree to `AllPathsExcluded`, `ProjectEvalCache::update` runs `apply_applicability_change` and then falls through to `update_step`, which rebuilds the tree's `Err(TreeNotApplicable)` slot; verified bit-identical to `ProjectEvalCache::new` across random interleaved edits and inverses in `applicability_cache::mixed_attacker_profile_scaling_conditions_assumptions_and_tree_edits`, and `gate.md` passed (`283` counted lines, all CI/test/bench checks ok).
View: git -C factory/worktrees/137-attacker-profile diff 86c0746eb4f63e8a50ce16bf62bbc53a0fac2705..HEAD
## Findings
none
## Lesson
none
NEXT: human
