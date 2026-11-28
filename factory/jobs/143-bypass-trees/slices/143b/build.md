Commit: d452ec9 Evaluate bypass mitigation ceilings, diff closure, and cache (143b)
Files: crates/aegis-model/src/control.rs, crates/aegis-model/src/project.rs, crates/aegis-model/tests/common/fragment_oracle.rs, crates/aegis-model/tests/bypass_trees.rs
Done when:
1. Mitigated step ceilings, `StepCeiling::bypass`, `step_ceilings`, `residual_step_feasibility`, and D28 statuses reflect bypass trees while leaving `ControlRule` ceilings unchanged — `cargo test --locked -p aegis-model --test bypass_trees --test control_settings --test control_status --test shared_fragments` passed (bypass_trees: 5 ok, control_settings: 9 ok, control_status: 4 ok).
2. Incremental cache proptest matches fresh `Project` and the independent recursive oracle across bypass edits, `SetStepMitigation`, and status flips — `cargo test --locked -p aegis-model --test bypass_trees --test control_settings --test control_status --test shared_fragments` passed (bypass_trees: 5 ok including 64-case proptest, shared_fragments: 35 ok).
3. Model crate passes fmt, clippy `-D warnings`, and all tests — `scripts/ci.sh crates/aegis-model` passed (`ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test`).
Notes:
- Diff size outside `crates/aegis-model/tests/`: +9/-1 in `crates/aegis-model/src/control.rs`, +282/-88 in `crates/aegis-model/src/project.rs` (+291 added lines <= 320 cap).
- Extracted `fold_mitigations` and `entry_rule_ceiling` so `entry_step_feasibility_for` and `entry_local_ceiling` share mitigation and `ControlRule` folding while preserving `None` (`MissingLeafValue` / `EmptyGate`) precedence over `Lifted::Excluded` regardless of sibling mitigation order.
- `Project::diff` checks `tree_dependency_closure` on both baseline and current trees and parameterizes `tree_affected_by_controls` by `from_other` so `controls.added()` / `controls.removed()` are checked in the matching project direction.
- `ProjectEvalCache` replaces `has_fragment_references()` in applicability invalidation with `has_tree_dependencies()`, extends `cached_roots` and `enqueue_users(..., residual, ...)` for counting bypass controls, and handles `Op::SetControlBypass` and `Op::SetStepMitigation` rank updates cleanly.

## Rework 1
Commit: be41982 Simplify bypass evaluation and diff helpers under cap (143b)
Files: crates/aegis-model/src/project.rs
Gate fix:
- `too big to review: 380 changed lines, cap 320`: consolidated duplicate bypass iteration and evaluation into `entry_bypass_trees`, `bypass_root_value`, `entry_local_ceiling(..., roots: Option<&FragmentRoots>)`, `tree_closure_affected`, and `enqueue_mitigated_by` without suppressing `cargo fmt`. Total diff outside `crates/aegis-model/tests/` against `26b37d2` is now +9/-1 in `control.rs` and +243/-64 in `project.rs` (317 changed lines <= 320 cap).
Checks:
- `cargo fmt --all`: passed.
- `scripts/ci.sh crates/aegis-model`: passed (`ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test`).
- `cargo test --locked -p aegis-model --test bypass_trees --test control_settings --test control_status --test shared_fragments`: passed (bypass_trees: 5 ok, control_settings: 9 ok, control_status: 4 ok, shared_fragments: 35 ok).
NEXT: reviewer
