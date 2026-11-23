VERDICT: PASS
## For the owner
What changed: Added `crates/aegis-client/src/exclusion.rs` with `reasons_text` and `ConditionSnapshot`, and wired `table.rs` (which feeds both table rows and the inspector banner) to use `reasons_text`. Updated `session.rs` to show the `ConditionSnapshot` undo toast after `Origin::Edit` condition switches when no control-setting toast applies. Added integration tests `excluded_reasons.rs` and `condition_toast.rs` covering all reason phrases, banner/row equality, and toast formatting.
Look at:
- `crates/aegis-client/src/exclusion.rs:52` — `reasons_text` formatting and empty-target fallbacks across `Reason` variants.
- `crates/aegis-client/src/exclusion.rs:147` — `ConditionSnapshot::capture` and `toast` step-delta and moved-risk computation.
- `crates/aegis-client/src/session.rs:2713` — `apply_edit_as` condition-switch toast fallback behind `setting_snap`.
Risk: none — all logic is pure client view/toast formatting over existing model queries (`applicability()`, `evaluate_residual_risk()`), verified by the script gate (`scripts/ci.sh crates/aegis-client`, `excluded_reasons`, `condition_toast`) and `build.md` test runs (`table_features`, `inspector_view`, and `aegis-workbench` `table_view`/`inspector_view`).
View: git -C factory/worktrees/135-applicability-ui diff 3b9abd6e45a69455c0eb7beef418c577a0f6d3f1..HEAD
## Findings
none
## Lesson
none
NEXT: human
