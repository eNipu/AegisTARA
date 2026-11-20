VERDICT: PASS
## For the owner
What changed: Added `TableTab` in `crates/aegis-workbench/src/app/table_tab.rs`, a `ColumnSpec`-driven GPUI table dock panel holding rows, visible/all headings, `TableFilters`, and clamped `GridSelection` with click/Shift+Click selection and inspector sync. Stored `Settings` directly on `Workbench` to back column visibility and developer source view without duplicating state, and wired all 8 `TableId` explorer folders in `Workbench::open`, `changed_skipping`, `close_tab`, and `files::replace` to open and refresh `TableTab` unconditionally. Added four headless GPUI tests in `crates/aegis-workbench/tests/table_view.rs` covering all 8 `Owner::Project` tables, shared `Problems` key normalization, emptied `Owner::Unassigned` refresh and selection clearing, and Settings-backed column visibility persistence.
Look at:
- `crates/aegis-workbench/src/app/table_tab.rs:110` — `TableTab::refresh` and `clamp_selection`, verifying filtered row recomputation and zero-dimension selection clearing.
- `crates/aegis-workbench/src/app/workbench.rs:332` — `refresh_tables`, `set_table_column_visible_skipping`, and `table_title`, verifying skip-before-read GPUI lease safety, disk persistence, and fallback titles when a key leaves `ExplorerIndex`.
- `crates/aegis-workbench/src/app/workbench.rs:514` — `Workbench::open` and `changed_skipping`, verifying all 8 table folders open and refresh unconditionally without filtering through `ExplorerIndex`.
Risk: Low — GPUI entity double-lease during tab refresh was checked in `refresh_tables` (guards `skip != Some(e.entity_id())` before `e.read(cx)`) and exercised by `workbench::tests::apply_edit_skipping_avoids_double_lease_and_refreshes_siblings`; missing `ExplorerIndex` entries (`Owner::Project` tables and emptied `Owner::Unassigned`) are covered by dedicated tests in `table_view.rs`.
View: git -C factory/worktrees/189-table-tabs diff 9dc3c6acb9d65aea44a5ca2b864716f010f26032..HEAD
## Findings
none
## Lesson
none
NEXT: human
