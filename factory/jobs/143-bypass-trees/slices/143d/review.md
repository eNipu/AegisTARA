VERDICT: PASS
## For the owner
What changed: `Workbench` now remembers the latest live attack-tree or step selection across inspector switches and clears it on workspace replacement. `InspectorPanel` adds a `"Create from selection"` button on the `Control` `"Bypass tree"` row that clones the remembered tree or subtree into a new `TreeRole::Fragment` bypass tree, disabled when read-only or when no live tree/node selection exists. Headless GPUI tests cover empty selection, retained selection, read-only refusal, fragment creation, explain-why updates, and undo/delete invalidation.
Look at:
- `crates/aegis-workbench/src/app/workbench.rs:471` — records `last_tree_selection` before the `skip` guard and validates liveness against the current `Project` in `last_tree_selection()`.
- `crates/aegis-workbench/src/app/inspector.rs:256` — avoids a GPUI double-lease by passing `Some(cx.entity_id())` as `skip` before rebuilding the control inspector view.
- `crates/aegis-workbench/src/app/inspector.rs:474` — wires the `"Create from selection"` button onto the `CONTROL_BYPASS` row with `!read_only && last_tree_selection().is_some()`.
Risk: none — all paths are guarded by `refuse_read_only` and `last_tree_selection()` liveness checks, and `window.draw` + `create_bypass_from_selection` headless GPUI tests and `scripts/ci.sh` passed.
View: git -C factory/worktrees/143-bypass-trees diff 1fdb046f8ffc4609fac8b74fbd54de76d2735763..HEAD
## Findings
none
## Lesson
none
NEXT: human
