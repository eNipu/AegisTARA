# EGM Spec Audit — `141-shared-steps` (Shared steps, BLK-01)

Synthesized from Tier 2 EGM Spec Audit (`egm-codebase-details` + `egm-implementation-readiness`).

## Architectural Alignment (`docs/decisions.md`)
- **Pass (no `needs-human` contradiction):**
  - **D29 / D30 / D32 / D33:** Consistent (`SharedStep` by reference inside project, single-undo `Edit::MakeShared` / `Edit::Unshare`, `PageGroup::Feasibility`, `RELATIONS` registry, `shared/steps.json` workspace split file).
  - **D37 (Workbench instead of Slint pages):** Keeping `aegis-shell` (`app.slint` `nav-entries`, `controller.rs => None`, `.po`) at nav/i18n parity while putting real UI in `aegis-client` view models and `aegis-workbench` (`TableId::SharedSteps`, `InspectorPanel` `shared_note`) matches jobs `135`–`137` and `factory/backlog.md` D37 policy.

---

## Blocking Defects (Must Fix Across `spec.md`, `plan.json`, and `prompts/141*.md`)

### B1. `ModelError::NodeIsRated` collision (`spec.md`, `141c.md`)
- `crates/aegis-model/src/project.rs:8656` (`check_remove_tree_node`) **already defines and uses `ModelError::NodeIsRated(NodeId)`** (tuple variant).
- `141c.md` Do #1 asks to add `ModelError::NodeIsRated { tree: TreeId, node: NodeId }`, which will fail to compile due to duplicate variant name / mismatched shape.
- **Required fix:** Either introduce a distinct variant name (e.g., `ModelError::StepIsRated { tree: TreeId, node: NodeId }`) or reuse existing `ModelError::NodeIsRated(NodeId)` consistently in `spec.md` and `141c.md`.

### B2. Cross-slice compilation & `undo_every_op.rs` / `ops_properties.rs` CI failures (`plan.json`, `141a`–`141f`)
- `crates/aegis-client/tests/undo_every_op.rs:28-38` (`declared_variants()`) parses `crates/aegis-model/src/op.rs` via `include_str!` and **fails CI on `aegis-client` whenever any `Op` variant in `op.rs` is not covered by a walk**.
  - `141a` adds `Op::{AddSharedStep, RenameSharedStep, RemoveSharedStep}` and runs `"ci": ["crates/aegis-model", "crates/aegis-client"]`, without `crates/aegis-client/tests/*` or `crates/aegis-client/src/undo_label.rs` in `allow`!
  - `141c` adds `Op::{UseSharedStep, UnuseSharedStep}`, and `141e` runs `"ci": ["crates/aegis-client", "crates/aegis-workbench"]` while deferring `undo_every_op.rs` to `141f` — so `141e` CI will fail!
- Exhaustive matches across `aegis-client` / `aegis-workbench` / `aegis-shell` / `aegis-server`:
  - Check all exhaustive matches on `Op`, `ElementRef`, `EntityKind`, `ProjectDiff`, `Selection`, `ElementKind`, `Edit`, `Inspector` struct literals (`crates/aegis-client/src/view.rs` `Inspector` has all `pub` fields — verify whether `aegis-shell` or tests construct `Inspector { .. }` or destructure it).
  - Ensure every slice keeps the crates listed in its `plan.json` `"ci"` green, and if `Op` variants are added in `141a`/`141c`, either:
    1. Keep `"ci": ["crates/aegis-model"]` in model-only slices (`141a`..`141d`, except `141b` which migrates `StepTarget` across `aegis-client` / `aegis-server` / `aegis-shell` if needed — note if `141b` runs `aegis-client` CI, `undo_every_op.rs` and `undo_label.rs` must already cover `141a`'s ops!), **OR**
    2. Update `undo_label.rs`, `commands.rs` (`Edit` + `undo_every_op.rs` walks), etc. in the exact slice before `crates/aegis-client` appears in `"ci"`.
    *(Verify carefully: if `141b` runs `"ci": ["crates/aegis-model", "crates/aegis-client"]`, then `aegis-client` tests WILL run in `141b`, which means `undo_every_op.rs` WILL check `AddSharedStep`/`RenameSharedStep`/`RemoveSharedStep` added in `141a`! Inspect `undo_every_op.rs` and every crate referencing `Op::SetStep*` before finalizing slice boundaries and `allow`/`ci` lists.)*
- Also note deterministic `proptest` ChaCha RNG in `crates/aegis-model/tests/ops_properties.rs` (`ops_generator_covers_every_op_and_most_errors`): whenever `crates/aegis-model/tests/common/mod.rs` adds generator arms or `ModelError` / `Op` variants change (`141a`, `141b`, `141c`), include `crates/aegis-model/tests/*` in `allow` and `--test ops_properties` in `checks` so kind-coverage thresholds stay accurate and passing.
- Note on `plan.json` `"ci": []` in `141f`: `factory/verify.py` treats `"ci": []` as full-workspace `./scripts/ci.sh` (all crates), which is correct for the final slice.

### B3. Invalid `NavEntry` struct fields in `141f.md` (`crates/aegis-shell/ui/app.slint`)
- `crates/aegis-shell/ui/app.slint:44-48` defines `struct NavEntry { label: string, icon: image, group: int }` (lines 530–549 use `{ label: @tr(...), icon: @image-url(...), group: ... }`).
- `141f.md` Do #6 specifies `{ title: @tr("Shared building blocks"), icon: @image-url("icons/workflow.svg"), group: 3, key: "page-shared-blocks" }`. Neither `title` nor `key` exists on `NavEntry`, which breaks Slint compilation.
- **Required fix:** Use `{ label: @tr("Shared building blocks"), icon: @image-url("icons/workflow.svg"), group: 3 }` and verify group index and `Pages` constants in `app.slint`.

### B4. Missing `ProjectEvalCache::update` & `may_change_applicability` in `141b.md` and `141c.md` (`crates/aegis-model/src/project.rs`)
- **`141b.md`:** `ProjectEvalCache::update` (`project.rs:14284-14305`) destructures `{ tree, node, .. }` on `SetStepFeasibility`, `SetStepFactor`, `SetStepVia`, and `SetStepAttackType`. `141b.md` omits `ProjectEvalCache::update` from `Read first` and `Do`: specify exact `StepTarget::Step { tree, node }` vs `StepTarget::Shared(_)` handling in `141b`, plus explicit instructions for `may_change_applicability` and `undo_label.rs::parts` + `target_of` (note: function is `target_of` at `undo_label.rs:232`, NOT `selection_of`).
- **`141c.md`:** `ProjectEvalCache::update` does not specify `Op::AddSharedStep` / `Op::RemoveSharedStep`! If `AddSharedStep` grows `project.shared_steps()` past `shared_users.len()` and `UseSharedStep` indexes `shared_users[idx]`, it will **panic out-of-bounds**. Also specify whether `UseSharedStep`, `UnuseSharedStep`, `SetStepVia { target: StepTarget::Shared(_), .. }`, and `SetStepAttackType { target: StepTarget::Shared(_), .. }` update control/applicability caches (`update_control_target` / `update_control_rule` / `update_control_status` or tree invalidation) for every using tree.

### B5. Missed `SharedStep` resolution sites in `crates/aegis-model/src/project.rs` (`141c.md`)
- `tree_affected_by_controls` (`project.rs:6724-6756`) iterates `entry.vias()` directly (`:6743`) instead of resolving through `step_via(tree_idx, node_idx)`. `Project::diff` (`diff_risk_deltas`) will miss risk deltas when a control affecting a `SharedStep`'s `via` item changes, or when a `SharedStep` itself changes ratings! Verify both `tree_affected_by_controls` and `diff_risk_deltas` when `SharedStep` ratings change.
- `lint_trees` (`project.rs:6068-6092`) reads `entry.step_factors` and `entry.step_feasibility` directly for `AttackTreeStepIncompleteFactors` and `RedundantStepFeasibility`. On linked steps those local fields are empty while values live on `SharedStep`. Explicitly decide and specify in `spec.md` + `141c.md` how `lint_trees` (and/or `SharedStep` lint) handles linked steps and shared steps.

### B6. Wrong symbol & constant names in `spec.md`, `141b.md`, `141e.md`
- `spec.md` line 32 & `141e.md` Do #2 cite `RelationKey::STEP_THROUGH` — the actual constant in `crates/aegis-client/src/relations.rs:44` is `RelationKey::STEP_VIA` (`"step-via"`). Also point out reusing `ITEM` (`relations.rs:146`) for `SHARED_STEP_VIA`.
- `141e.md` Do #5 cites `page_of` in `crates/aegis-workbench/src/app/workbench.rs` — `workbench.rs` has no `page_of`; its exhaustive `Selection` match is `fn kind(names: &ElementNamesDef, sel: Selection) -> Option<String>` (`workbench.rs:163-179`). (`page_of` is in `crates/aegis-client/src/pages.rs:3324`.)
- `141b.md` Do #4 & `141e.md` Read first cite `undo_label.rs::selection_of` — the actual function at `crates/aegis-client/src/undo_label.rs:232` is `target_of`.
- `141b.md` Read first / Do #3 attributes `OpWire` to `store/op_log.rs` — `OpView`, `op_view`, and `op_from_wire` are in `op_log.rs` (`241-269`, `1134-1179`, `2130-2178`), while `OpWire` is in `store/wire.rs:2360` (`SetStep*` at `wire.rs:2540-2576`).
- `crates/aegis-client/tests/relations.rs:29-49` (`Sel::key`) matches `Selection` exhaustively — document adding `Selection::SharedStep` in `141e.md`.

### B7. Missing `commands.rs` cleanup on `RemoveTreeNode`, `RemoveTree`, and `RemoveSharedStep` (`141e.md`)
- `Edit::RemoveTreeNode` (`commands.rs:2581` -> `tree_diff::remove_node_ops`) and `Edit::RemoveTree` (`commands.rs:2401-2434`) will fail with `NodeUsesSharedStep` unless `clear_step_ops` (which emits `UnuseSharedStep` when linked) is invoked for every removed step (including gate subtrees removed via `RemoveTreeNode` / `ReplaceTree`).
- `clear_descriptions_before_removal` (`commands.rs:1241-1260`) must map `Op::RemoveSharedStep { id } => Some(E::SharedStep(*id))` so `Edit::RemoveSharedStep` clears descriptions/translations before removing.

### B8. Left-to-builder specifications across `141a`–`141f`
Eliminate all ambiguity so builders make zero design choices:
1. **`141a.md`:**
   - Specify exact `SharedStep` derives, constructor, `factors` representation + allocation semantics, and exact `is_rated(&self) -> bool` formula (`feasibility.is_some() || factors.iter().any(Option::is_some) || via.is_some() || attack_type.is_some()`). Re-export `SharedStep` directly in `141a`.
   - Specify exact `Project` accessor signatures/return types (`shared_steps`, `shared_step`, `shared_step_idx`, `shared_step_id`, `shared_step_visual_id`, `shared_step_count`), `MAX_SHARED_STEPS` error, exact guard check order in `check_add/rename/remove_shared_step` (including description/translation check via `require_no_description` or equivalent), and `Op::provenance`/`with_provenance` on `RemoveSharedStep`.
   - **L29:** Explicitly require `Project::eq` to compare `shared_steps` by public `SharedStepId` and `via` by public `ItemRef` (never raw `ItemIdx`).
   - Specify exact `OpView`/`OpWire`/`TargetWire` JSON field names and `within_caps` checks, and decide definitively where `Limit::SHARED_STEPS` is added (`141a` or `141d`, not "if needed").
2. **`141b.md`:**
   - Specify `SharedStep.factors` allocation size when setting `Some(level)` from empty, exact `StoreError` variant/message for invalid `(tree, node, shared_step)` wire combinations, and `OpWire`/`OpView` field types + `within_caps`.
3. **`141c.md`:**
   - Specify exact signatures for `step_shared_id`, `TreeEntry::shared_at`, `set_shared`, exact check order and gate-node rejection error variant in `check_use_shared_step`/`check_unuse_shared_step`, deterministic `(tree, node, shared_step)` scan order on `RemoveTree` / `RemoveTreeNode` (including descendant steps under removed gates), and exact JSON field names on `OpView`/`OpWire` for `UseSharedStep`/`UnuseSharedStep`. Remove unused `store/wire.rs` from `141c` `allow` if untouched.
4. **`141d.md`:**
   - Specify exact `SharedStepWire`/`SharedStepView` (`factors` shape), `StepSharedEntry` field names, sort order (`NodeId`), `within_caps` bound, and duplicate-node rejection.
   - Specify exact root JSON schema of `shared/steps.json` in `workspace.rs` (matching `141f`'s `table_source_json`), `blame` mapping, and exact `snapshot.rs::load` replay order (**after** `methodology` and `items` [components/interfaces], **before** `attack_trees`, `descriptions`, and `translations`).
5. **`141e.md`:**
   - Specify exact field names and types for all 11 `Edit` variants (`AddSharedStep`, `RenameSharedStep`, `RemoveSharedStep`, `UseSharedStep`, `UnuseSharedStep`, `SetSharedStep*`, `MakeShared`, `Unshare`), `None`-skipping behavior in `MakeShared` / `Unshare` / `RemoveSharedStep`, exact `RelationKey::SHARED_STEP_VIA` & `SHARED_STEP_USED_IN` key strings, label keys, and deterministic sort order, exact Fluent keys + placeholders + EN/JA/pseudo strings for `inspector-shared-step-line*`, `Selection::SharedStep` inspector fields, and exact stub return values in `picker.rs`, `pages.rs`, `table.rs`, `systems.rs`, `grid/edit.rs`.
6. **`141f.md`:**
   - Replace vague `"description kind, etc."` with exact hyphenated lowercase Fluent keys (L27), noting existing reusable keys (`feasibility`, `through`, `attack-type`, `inspector-used-in`).
   - Specify `Folder::SharedSteps` Fluent key and ordering under `Folder::SharedAnalysis`, concrete `table_chrome.rs` / `table_edit.rs` arms for `TableId::SharedSteps`, and either specify the `crates/aegis-client/src/state.rs` change or remove `state.rs` from `allow`.

### B9. Falsifiable Test & Factory Lessons Gaps (`141a`–`141f`)
- **L15 (blank-actor provenance):** Explicitly require blank-actor rejection tests for `AddSharedStep` & `RenameSharedStep` (`141a`), all 4 `SetStep*(StepTarget::Shared)` ops (`141b`), and `UseSharedStep` & `UnuseSharedStep` (`141c`).
- **L09 (single-guard isolation) & L20 (all variants/branches):**
  - `141a`: isolated tests for `DuplicateSharedStep`, `UnknownSharedStep` (rename & remove), `MAX_SHARED_STEPS`, `check_name`, and `SharedStepInUse` (with `is_rated() == false` and no description).
  - `141b`: `UnknownSharedStep` on all 4 ops, isolated `SharedStepIsRated` for each of the 4 fields individually + clearing factors back to empty, and all invalid `(tree, node, shared_step)` wire combinations.
  - `141c`: `RemoveTree` rejection (`NodeUsesSharedStep`), all 4 `SetStep*(StepTarget::Step)` rejections (`NodeUsesSharedStep`), isolated `StepIsRated`/`NodeIsRated` for each of the 4 own fields individually, `AddSharedStep` -> `UseSharedStep` `ProjectEvalCache` slot-growth test, and independent hand-computed expected values (feasibility, residual feasibility, control ceilings, applicability, `lint_trees`, `diff_risk_deltas`, `SystemUse`) rather than only comparing cache vs fresh eval.
  - `141d`: isolated `Limit::SHARED_STEPS` (`MAX_SHARED_STEPS + 1` only), `deny_unknown_fields` tests for `SharedStepWire`, `StepSharedEntry`, and `shared/steps.json`, plus `ElementRef::SharedStep` description/translation + linked-tree round-trip without duplicating shared ratings onto `TreeWire`.
  - `141e`: distinct-tree singular vs plural test (2 linked nodes in **1** tree -> `"Shared step L01, used in 1 tree"`, vs 2 trees -> `"used in 2 trees"`).
  - `141f`: L21 `matches_owner` test covering `Owner::Unassigned` (empty), `Owner::Project` (all), and `Owner::System(s)` (only shared steps used by `s`).
- **L30 (removal/unlink + undo + full `Project` equality + eval/refcount re-check):** Require explicit undo `assert_eq!(project, before)` + refcount/eval/cache assertions on `RemoveSharedStep`, `UnuseSharedStep`, `Edit::MakeShared`, `Edit::Unshare`, and tree diff move/remove of linked steps.

### B10. Realistic `max_lines` Caps (L14 / L28)
Raise tight `max_lines` budgets in `plan.json` (or split slices if >750) so builders never resort to formatting compression (L28):
- `141a`: `520` -> ~`620`
- `141b`: `540` -> ~`700` (check exact scope across `aegis-client`/`aegis-server` if included)
- `141c`: `540` -> ~`720`
- `141d`: `360` -> ~`460`
- `141e`: `560` -> ~`720`
- `141f`: `560` -> ~`660`
- Also fix drifted `Read first` line ranges noted by `egm-codebase-details` (`project.rs` `RefCounts` `:375-411`, `Assumption` ops `:9974+`, `PartialEq` `:12764+`, `check_invariants` `:13089+`, `lint_trees` `:6025-6100`, `tree_affected_by_controls` `:6724-6756`, `snapshot.rs` `:25-80`, `pages.rs` `comparison_in` `:3562-3821`).
