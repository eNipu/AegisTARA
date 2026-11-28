# EGM Spec Audit: `143-bypass-trees`

Audited sequentially by `egm-codebase-details` and `egm-implementation-readiness`.

## Blocking Defects (must be fixed by `planner` before Slice 1)

1. **`143c` exceeds D41 `max_lines <= 400` and mixes `ci: []` (`plan.json`, `prompts/143c.md`)**
   - `docs/decisions.md` (D41) limits slices to at most 400 changed lines. `plan.json` sets `"max_lines": 490` for `143c` across `aegis-canvas`, `aegis-client`, and `aegis-workbench`.
   - **Fix**: Split `143c` into two slices (`143c` and `143d`), each comfortably `<= 400` lines (e.g., `143c` for `aegis-canvas` + `aegis-client` core/explain-why/relations/commands/pages/view/locales, and `143d` for `aegis-workbench` GPUI + final full-workspace `"ci": []`).

2. **Public struct field `allow` gaps and missing literal updates (`ControlDetail`, `NodeStyle`, `Op`, `Edit`)**
   - `ControlDetail::bypass` (`crates/aegis-client/src/pages.rs`) and `NodeStyle::excluded_label` (`crates/aegis-canvas/src/scene.rs`) are public required fields.
   - `prompts/143c.md` missed the early-return `ghost_overlay` `NodeStyle { ... }` literal in `crates/aegis-client/src/view.rs:1671-1682`.
   - Audit all workspace construction/destructure sites of `ControlDetail`, `NodeStyle`, `SecurityControl`, `Op`, `ModelError`, and `Edit` (including `crates/aegis-client/tests/*`, `crates/aegis-workbench/src/*`, `crates/aegis-shell/*`) and ensure every affected file is in the slice's `allow` list (Lesson L24). Also verify workspace compile safety between `143a` (`Op::SetControlBypass`) and `143c`/`143d` (e.g. `crates/aegis-client/src/undo_label.rs` or any exhaustive `Op` matchers).

3. **Inconsistent `"bypass Medium"` format in `prompts/143c.md` vs `spec.md`**
   - `spec.md` and `143c.md` acceptance tests require `"bypass Medium"` (no colon), whereas `143c.md` instructed `tree_scene` (`view.rs`) to emit `"{name} (bypass: {})"` (`"bypass: Medium"`), `control_uses` (`pages.rs`) to append `" (bypass)"`, and `reduction` (`pages.rs`) to put `"bypass"` in `on` while `ceiling` holds `"Medium"`.
   - Align `spec.md` and the slice prompt(s) so `reduced_by`, step explain-why, threat why lines, `tree_scene` shield tooltip/capped label, and `control_uses` format unambiguously and match every test assertion.

4. **`aegis-canvas` accessibility implementation & test missing for `"control off"` (`spec.md` AC #6)**
   - `prompts/143c.md` did not inspect where `NodeStyle::excluded` / `excluded_label` feeds accessible text in `aegis-canvas` (e.g., `crates/aegis-canvas/src/access.rs` vs `scene.rs`/`render.rs`) and lacked a test asserting accessible text carries `"control off"`.

5. **`Selection::Control` `"Bypass tree"` cache call and `TreeNotApplicable` fallback ambiguous (`prompts/143c.md`)**
   - Specify the exact `ws.cache()` method (`evaluate_residual_tree` vs `evaluate_tree` vs status-dependent) and the exact display/tone when the bypass tree evaluates to `TreeNotApplicable` (`Excluded`). Also verify `InspectorPanel::render_row` signature and struct fields (`self.workbench`, `apply_built`) in `crates/aegis-workbench/src/app/inspector.rs`, and verify `Edit::unchanged` for `CreateBypassFromSelection`, `DuplicateControl`, `undo_label.rs`, and tests for `ControlDetail::bypass`, `control_uses`, `diff_lines`, and fragment reverse relation (`"Bypass tree for"`).

6. **`143a` `check_fragment_graph` candidate injection & out-of-bounds panic (`crates/aegis-model/src/project.rs:1582-1604`)**
   - `entry.step_mitigations` is lazily sized (`project.rs:4041` uses `.get(node.index())`), so direct indexing `entry.step_mitigations[node.index()]` will panic on trees with no mitigations.
   - When `SetStepMitigation` attaches `control` to `(idx, node)` for the first time, `control` is not yet in `entry.step_mitigations`, so iterating existing mitigations misses the newly added bypass edge. Must inject the candidate `(idx, node, control)` when not already present.

7. **`143a` non-existent `self.tree_index(tid)` and `common/mod.rs` generator / `record_every_op` / `blame` defects**
   - Use `self.live_tree(tid)?.0` instead of non-existent `self.tree_index(tid)`.
   - `crates/aegis-model/tests/common/mod.rs:678` uses `0u8..92`, arm `83` is missing, and line `2479` ends with `_ => Op::SetComponentSystem { ... }`. Specify the exact arm placement so both `SetControlBypass` and `SetComponentSystem` are reachable and `ops_properties` expectations match.
   - In `record_every_op`, clearing `SetControlBypass` before `RemoveAttackTree` / `RemoveSecurityControl` must be mandatory (not optional).
   - Specify exact `store/workspace.rs` `blame` behavior for bypass-related `ModelError` variants (`UnknownSecurityControl`, `UnknownAttackTree`, `FragmentRole`, `FragmentCycle`, `FragmentDepth`) so `"shared/controls.json"` is blamed appropriately instead of falling through to `"aegis.json"`.

8. **`143b` broken `entry_local_ceiling` / `enqueue_users` / `Project::diff` / `deciding_step` specs**
   - `entry_local_ceiling` (`project.rs:4303-4341`) currently returns `FeasibilityLevel` and has no `roots: &FragmentRoots`, and is shared by `lifted_leaf` and `residual_step_feasibility`. Specify the exact signature/helper so `own`/`shared` rating is still checked before mitigations on non-fragment steps, and define exact precedence between `Lifted::Excluded` and `None` (`MissingLeafValue`) when a step has multiple bypassed mitigations — identically in `entry_local_ceiling` and `tests/common/fragment_oracle.rs`.
   - `ProjectEvalCache::enqueue_users` (`project.rs:16351-16389`) must read `project.controls` (not non-existent `self.controls`), accept `residual: bool` (or equivalent), and update **all** callers including `update_tree_structure` (`project.rs:16498+`, add to `Read first` and `Do`) so structural edits inside a bypass tree invalidate bypass-using steps. Also clarify `update_control_mitigations` in `Read first`/`Do`.
   - `Project::diff` (`project.rs:7168-7338`): `diff_risk_deltas` and `is_threat_affected` must be in `Read first` and updated to pass `app_a`/`app_b` (since `merge_condition_link_changes` runs after `diff_risk_deltas`) and traverse full `UseFragment` + `ctrl.bypass` closures (including controls mitigating steps inside bypass trees and sub-fragments) in both `self` and `other`.
   - Explicitly state whether `deciding_step` (`project.rs:4178-4210`) is updated or unchanged.

## Non-Blocking Notes

- Check `143a` `max_lines: 340` against Lesson L14 (8 source files + doc comments); raise up to `<= 400` if needed so builder isn't squeezed.
- Clarify `crates/aegis-model/src/error.rs:620` (`FragmentInUse` message) if tests check exact text when a fragment is used as a bypass.
- Consider adding `SetStepMitigation` mutations and `step_ceilings` / `residual_step_feasibility` oracle checks to `143b` proptests.
