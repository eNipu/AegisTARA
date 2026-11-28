VERDICT: PASS
## For the owner
What changed: Added `excluded_label` hatching/accessibility in `aegis-canvas`, registered the `"Bypass tree"` relation (`CONTROL_BYPASS`), `SetControlBypass`, `CreateBypassFromSelection`, and bypass-clearing `RemoveControl` edits in `aegis-client`, and surfaced `"bypass <Level>"` ceilings, `ControlDetail::bypass`, diff lines, and `"control off"` canvas hatching across inspector, pages, and `tree_scene`.
Look at:
- `crates/aegis-client/src/view.rs:1666-1711` — `control_off` detection and precedence between applicability exclusions and inactive bypass trees.
- `crates/aegis-client/src/commands.rs:3186-3218` — `CreateBypassFromSelection` subtree copying (`PreserveReferences`) and single-undo linking.
- `crates/aegis-canvas/src/access.rs:102-105` — combining `excluded_label` and applicability `excluded` reason without duplicating `"control off: control off"`.
Risk: none (all edge cases—applicability exclusion overriding `"control off"`, `UseFragment` suppressing `"control off"` hatching, `RemoveControl` and `CreateBypassFromSelection` single-step undo round-trips, and every `"bypass <Level>"` surface—are covered by `bypass_trees.rs`, `inspector_view.rs`, `relations.rs`, `undo_every_op.rs`, and `aegis-canvas` tests).
View: git -C factory/worktrees/143-bypass-trees diff be419820e37cffa94a28f1a31367355dbe64a4a6..HEAD
## Findings
none
## Lesson
none
NEXT: human
