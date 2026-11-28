VERDICT: PASS
## For the owner
What changed:
Step mitigation ceilings now evaluate counting controls' bypass trees (replacing typed `StepMitigation::feasibility` while leaving `ControlRule` ceilings untouched) and expose `StepCeiling::bypass()`.
`Project::diff` and `ProjectEvalCache` now traverse and incrementally invalidate bypass-tree and sub-fragment closures across status changes, bypass swaps, applicability toggles, and step edits.
An updated recursive oracle and `bypass_trees.rs` test suite (including a 64-case proptest with undo/redo/reload) verify live evaluation, `step_ceilings`, `residual_step_feasibility`, diff deltas, and incremental caching.
Look at:
- `crates/aegis-model/src/project.rs:4418` — `entry_local_ceiling` folds bypass roots so `MissingLeafValue`/`EmptyGate` (`None`) strictly beats `Lifted::Excluded` regardless of sibling mitigation order, and `step_ceilings` (`project.rs:4147`) omits unresolvable/excluded bypass ceilings instead of falling back to `m.feasibility`.
- `crates/aegis-model/src/project.rs:7400` — `tree_closure_affected` walks fragment and bypass closures in both baseline and current trees and parameterizes `tree_affected_by_controls` direction (`from_other`) for `added`/`removed` controls.
- `crates/aegis-model/src/project.rs:16646` — `enqueue_users` and `update_control_mitigations` (`project.rs:17066`) propagate bypass root updates incrementally in dependency order while `ProjectEvalCache::update` rebuilds when applicability changes with active tree dependencies.
Risk:
A subtle mismatch between `entry_local_ceiling` (`None` vs `Excluded` precedence), `step_ceilings`, `Project::diff`, and `ProjectEvalCache` incremental propagation across status transitions (`Implemented`/`Planned`/`Off`) or `SetControlBypass`/`SetStepMitigation` mutations; checked via targeted unit tests in `crates/aegis-model/tests/bypass_trees.rs` (4 deterministic integration tests covering D28, exclusion/unrated precedence, and 7 diff scenarios) plus a 64-case proptest verifying `ProjectEvalCache` against fresh `ProjectEvalCache::new`, live `step_ceilings`/`residual_step_feasibility`, and the independent `fragment_oracle::Oracle` after every op/undo/redo/save-load.
View: git -C factory/worktrees/143-bypass-trees diff 26b37d2eeb05da8d07e552c2927cc5c9736c5e53..HEAD
## Findings
none
## Lesson
none
NEXT: human
