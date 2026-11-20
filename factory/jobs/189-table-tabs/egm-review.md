# EGM Spec Audit: `189-table-tabs` (`189b2`..`189e2`)

Audited against live worktree `factory/worktrees/189-table-tabs/` (commit `4e796c6`, with `189a` and `189b1` already committed) using two fresh-context Goldfish personas (`egm-codebase-details` and `egm-implementation-readiness`).

## Blocking Findings to Fix Before Building `189b2`..`189e2`

### Codebase & API Defects (`egm-codebase-details`)
1. **GPUI double-lease panics across `189c1`, `189c2`, `189d1`, `189d2`, `189e1`, `189e2`**:
   - Calling `self.workbench.update(...)` while leasing `MultiLinkPickerView` or `TableTab`, when `Workbench::changed` / `set_table_column_visible` synchronously updates that same entity, panics in `EntityMap::lease`.
   - Calling `active_table_tab.update(...)` inside `Workbench::run` when that `TableTab` calls `self.workbench.update(...)` panics on the leased `Workbench`.
   - **Fix**: Add `skip: Option<EntityId>` plumbing (`apply_edit_skipping`, `changed_skipping`, `set_table_column_visible_skipping` on `Workbench`, and `skip: Option<EntityId>` on `InspectorPanel::set_selection` / `TableTab::refresh`) so the caller entity skips synchronous re-entry and refreshes itself immediately after `workbench.update(...)` returns. For `Workbench::run("grid-*")` in `189e2`, read `TableTab` state / build the `Edit` without re-entering `Workbench`, then call `self.apply_edit(&edit, cx)` directly in `Workbench::run`.
2. **`189b4` hardcoded `category: 0` violates `Project::check_add_threat`**:
   - Reuse `pub(crate) fn default_threat_category(project: &Project, asset: AssetId) -> usize` already added in `189b1` (`crates/aegis-client/src/grid/mod.rs:210-223`).
3. **`189b4` wrong `relations::candidates` arity**:
   - Signature is `relations::candidates(project: &Project, key: RelationKey) -> Vec<Selection>` (2 args, not 3).
4. **`189b4` nonexistent `AssetId::new()`**:
   - Use `::new_v7()` on all typed IDs (`AssetId::new_v7()`, `DamageId::new_v7()`, `ThreatId::new_v7()`, `ControlId::new_v7()`, `GoalId::new_v7()`, `ClaimId::new_v7()`).
5. **`189d1` three compile errors (`table_rows_for_owner`, `pages::vocabulary_for_lang`, missing `Catalog::language()`)**:
   - `table_rows_for_owner` takes 5 args `(ws, table, owner, &vocab, &catalog)`.
   - `pages::vocabulary_for_lang` takes `(&Methodology, &str)` (`ws.project().methodology()`), not `&Workspace`.
   - Add `pub fn language(&self) -> Language` to `Catalog` in `crates/aegis-client/src/i18n.rs` (add `crates/aegis-client/src/i18n.rs` to `189c1`'s `allow` in `plan.json`).
6. **`189d2` wrong `table_source_json` argument type**:
   - Signature is `table_source_json(project: &Project, table: TableId) -> String` — pass `ws.project()`, not `ws`.
7. **Private `table.rs` helpers & wrong path for `selection_visual_id`**:
   - `selection_visual_id` is in `crates/aegis-client/src/pages.rs`, not `table.rs`.
   - Add `crates/aegis-client/src/table.rs` to `189b4` (or `189c1`) `allow`, make `selection_label` and a `selection_exclusion(project, sel) -> (bool, Option<String>)` helper `pub`, and reuse them in `paste.rs` and `inspector.rs`.
8. **`189d1` `Workbench::open` & `Workbench::changed` silently drop `TableTab`s missing from `ExplorerIndex`**:
   - `Owner::Project` does not list `Assets`/`Threats`/`Risks` in `PROJECT_TREE`, and `Owner::Unassigned` disappears from `ExplorerIndex` when emptied.
   - Allow `Workbench::open` to construct `TableTab` with a fallback title when `table_for_folder(folder)` is `Some`, and refresh every `Body::Table` unconditionally in `Workbench::changed`.

### Behavioral, Safety & Readiness Defects (`egm-implementation-readiness`)
9. **DATA LOSS BUG (`189e2`) — `delete_selected` on `TableId::Problems`**:
   - `Problems` rows carry `row.target = Some(Selection::...)`; `delete_selected` MUST refuse `self.table == TableId::Problems` (and `source_mode`), with a dedicated regression test!
10. **No-op `Edit::Batch(vec![])` pollutes undo / clears redo (`189b3`, `189c1`)**:
    - Short-circuit empty ops in `local::apply_edit` (add `crates/aegis-workbench/src/local.rs` to `189c1` `allow`) or in `Workbench::apply_edit`; in `189b3` `fill_down`, check `matches!(&batch, Edit::Batch(v) if v.is_empty())` after `finish_batch` and return `Err(GridError::NothingToFill)`.
11. **`"Grid"` key context leaks into inline `InputState`s (`189c1`, `189d1`, `189d2`, `189e1`, `189e2`)**:
    - Only attach `.key_context("Grid")` when `!self.is_editing() && !self.source_mode && self.filter_col.is_none()`, and guard all 5 grid commands in `TableTab`/`Workbench::run` against editing/filtering/source mode.
12. **GPUI bubble `on_key_down` vs `capture_key_down` (`189c1`, `189e1`)**:
    - Use `capture_key_down` on `MultiLinkPickerView` (for `up`/`down`/`enter`/`escape`, leaving `space`/typing untouched when filter `Input` is focused) and on `TableTab` (so `Escape` cancels an active `InputState` editor).
13. **`189b2` `CellInput::Impacts` length & stale target**:
    - Reject `levels.len() != project.methodology().impact_category_count()` with `GridError::UnknownRating`; validate bounds against `project.methodology()`; return `Err(GridError::NotEditable)` for stale/missing targets; clarify `edit_cells` 1-based error row index.
14. **`189b3` `sync_link_edits`, `fill_down`, and `parse_tsv` edge cases**:
    - Deduplicate `desired` preserving order; for `!rel.many`, only emit `unlink` when `desired.is_empty()` and `cur` non-empty, else emit `(rel.link)` only if changed.
    - Clamp `end_row` and `col_range` to bounds and skip `target: None` in `fill_down`.
    - Specify `parse_tsv` RFC-4180 malformed quotes (`foo"bar`, `"foo"bar` -> `MalformedQuote { line }`), 1-based line counting across multiline fields, stripping at most one trailing `\r\n`/`\n` before empty check, and `format_tsv` empty/out-of-bounds return `String::new()`.
15. **`189b4` `Damage.impact` locale round-trip, `TREATMENTS` order, 1:1 multi-token paste, empty link on append**:
    - Match `Damage.impact` category & level labels case-insensitively against BOTH `vocab.impact_categories` AND `project.methodology()` default labels (replacement semantics on existing rows; only emit `Some` on appended rows).
    - Map treatments using `TREATMENTS` order `[Avoid(0), Reduce(1), Share(2), Retain(3)]`.
    - Reject `> 1` parsed link targets for `!rel.many` with `GridError::UnknownLink`.
    - Candidate matching precedence: exact `visual_id` -> `"visual_id — name"` -> exact `name` (first in candidate order).
    - On appended rows, empty `"component"` / `"asset"` cell falls back to `system_component` / `default_asset_for_owner`; ignore excess trailing pasted columns.
16. **`189c2` stale `InspectorPanel` selection & missing read-only guard on `open_picker`**:
    - Clear selection/picker in `InspectorPanel::set_selection` if element no longer exists in `project`, and clear on `Workbench::replace` (`files.rs`).
    - Guard `InspectorPanel::open_picker` against read-only workspaces.
    - Qualify `gpui::` paths in `dock_panel!` when exporting `pub(super) use dock_panel;`.
17. **`189d1` & `189d2` layout, `Settings` reuse, filter input sync, and hidden filtered columns**:
    - Use `overflow_scroll()` + `flex_shrink_0()` on fixed-width grid columns (`Risks` totals 1206 px).
    - Reuse `Settings::visible_columns` / `set_column_visible` directly in `Workbench` instead of duplicating `table_columns`. Normalize `Workbench::table_tab(key)` with `shared(key)`.
    - Guard programmatic `filter_input.set_value(...)` with a `syncing_filter` flag in `189d2`; look up active filter chip headings from `self.table.columns()` (not just `visible_cols`); close `filter_col` if hidden or entering `source_mode`; render a readable label (e.g. `"—"`) for empty `""` distinct values.
18. **`189e1` & `189e2` editor commit/cancel, `ImpactRatings` atomicity, and `add_row`/`paste` edge cases**:
    - `commit_input`: check read-only + `!source_mode`, fall back to `self.selection?.active_col` when `self.editor` is `None`, clear editor & restore focus ONLY on `Ok`.
    - `ImpactRatings`: edit local levels and commit `CellInput::Impacts` on Apply (one undo step).
    - Arrow/`tab` when `selection` is `None` selects `(0, 0)`; clicking another cell cancels any active editor; `LinkPicker` `on_close` restores focus to `TableTab`.
    - `add_row`: surface `NoAssetForThreat` in `Workbench::set_note`; do not select/edit an unrelated row if filters hide the new row or `"name"` is hidden.
    - `paste_tsv_text`: anchor at top-left `(*sel.row_range().start(), *sel.col_range().start())`, default `(0, 0)`.

### `max_lines` Sizing & Test Coverage Fixes
- Slices `189c1` (est. 450–560+), `189d1` (est. 560–650), `189d2` (est. 405–550), `189e1` (est. 550–630) risk exceeding the 600-line slice cap once double-lease fixes and all required behavior are included. Rebalance/split where needed so **every** remaining slice fits comfortably under `<= 600` counted lines (e.g., splitting `189c1` into `189c1a` commands/i18n/hooks + `189c1b` `MultiLinkPickerView`, and keeping `189d1`/`189e1` comfortably under 600 via clean modular split or `Settings` reuse).
- Add the missing falsifiable tests noted by `egm-implementation-readiness` (especially `Problems` delete refusal, `NoAssetForThreat` guard, `open_filter_for` switching without clobbering, `simulate_keystrokes` in `"Grid"` context, `InspectorPanel` stale-selection & read-only guard, and `palette.rs` non-regression).
- Add `"backspace" => "⌫"` and `"delete" => "Del"` arms to `shortcut_label` in `crates/aegis-client/src/command_table.rs` (`189c1`).
