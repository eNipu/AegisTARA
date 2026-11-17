VERDICT: PASS
## For the owner
What changed: Tree, threat and risk evaluation, both cached and uncached, now skip steps that a missing condition excludes. OR gates ignore an excluded step, and AND/SEQ gates are excluded along with it.
A threat that is excluded, or whose tree is excluded, now returns a new "not applicable" error instead of a level. Sensitivity, tipping points and attack paths follow the same rules.
When every condition is present, the numbers are the same as before. A test checks this, and the existing project/eval/explain suites pass unchanged.
Look at: crates/aegis-model/src/project.rs:14131: the incremental cache path syncs the plain view from the lifted cache and falls back to a full slot rebuild. A mistake here would give stale cached numbers. It is covered by `cache == ProjectEvalCache::new` after every op in ops_properties and in the toggle tests.
crates/aegis-model/src/project.rs:6460: `Project::diff` now also reports a threat as changed when its applicability differs. The prompt did not ask for this, but the diff-soundness property test needs it. It errs toward reporting too many changes, never too few.
crates/aegis-model/src/paths.rs:621: `matches_tree` now accepts nodes that have no value (excluded nodes). Without this, attack paths and choke points come back empty for any tree with an excluded step.
Risk: Speed, not correctness. Once any condition is absent, every uncached `evaluate_*` call and every `ProjectEvalCache::update` works out the applicability of the whole project again. If any condition changed, `update` then rebuilds the whole cache. The prompt asked for this, and slice 134c is due to make it incremental. When all conditions are present it returns early and costs almost nothing. Correctness was checked by the exact-value tests in applicability_eval.rs (OR, AND, SEQ, nested, unrated, threat-level, inherent and residual, cache toggles) and by the property test that compares cache and project after every generated op.
View: git -C factory/worktrees/134-applicability-engine diff 7197189b66074de7c57f34371411bfe8d2b4dbf4..HEAD
## Findings
- none. Every "Done when" item is met, and each has a test that would fail without the change. The nested test asserts that paths are non-empty and equal `[[c]]`. The OR test checks the exact root before and after exclusion. The all-present test compares risk, sensitivity and tree evaluations against a project with no conditions. The ops_properties edits keep each assertion's meaning: "lint clean implies Ok" now also allows "Ok or not applicable", and the tipping-point oracle uses the same lifted leaf.
- Minor, no change needed: crates/aegis-model/src/project.rs:14131. When `recompute_steps_recording` already rebuilt the lifted cache, the slot is rebuilt a second time through `build_lifted_tree`. This is duplicate work but gives the right result.
## Lesson
none
NEXT: human
