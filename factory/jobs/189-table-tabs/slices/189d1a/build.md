Commit: 7ff0162 Add GPUI TableTab grid state and wire table folders into Workbench (189d1a)
Files: crates/aegis-workbench/src/app/table_tab.rs, crates/aegis-workbench/src/app/workbench.rs, crates/aegis-workbench/src/app/files.rs, crates/aegis-workbench/src/app/mod.rs, crates/aegis-workbench/tests/table_view.rs
Done when:
1. All 8 `TableId` folders (including `Owner::Project` and emptied `Owner::Unassigned`) open/refresh via `TableTab` and column visibility persists — `cargo test --locked -p aegis-workbench --test table_view` -> PASS (4 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench` -> PASS (`fmt`, `clippy`, `test` all ok in `aegis-workbench`)
Notes:
- `TableTab` is driven purely by `ColumnSpec` and `TableRow` with `pub(super)` fields ready for 189d1b/189d2/189e1a/189e1b/189e2 (`#[allow(dead_code)]` on `all_headings` until 189d2 reads it for hidden-column filter chips).
- `Workbench` stores `settings: Settings` directly (removing the duplicate `developer_source_view` field) and delegates column visibility directly to `Settings::visible_columns` / `Settings::set_column_visible`, refreshing all open `Body::Table` tabs unconditionally on workspace edits and matching-table tabs on column visibility updates.
- Updated `workbench::tests::apply_edit_skipping_avoids_double_lease_and_refreshes_siblings` to match `Body::Table` since `Folder::Assets` now opens a `TableTab` rather than a `ListTab`.
- Source diff size excluding `tests/`: 462 insertions / 16 deletions (478 total, under the 480 cap).
NEXT: reviewer
