Commit: 8b09a1b Add Fragments table, project explorer folder, and grid wiring (142l)
Files:
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/src/explorer.rs
- crates/aegis-client/src/grid/edit.rs
- crates/aegis-client/src/grid/fill.rs
- crates/aegis-client/src/grid/mod.rs
- crates/aegis-client/src/grid/paste.rs
- crates/aegis-client/src/pages.rs
- crates/aegis-client/src/session.rs
- crates/aegis-client/src/table.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/tests/configuration.rs
- crates/aegis-client/tests/explorer.rs
- crates/aegis-client/tests/grid_edit.rs
- crates/aegis-client/tests/shared_fragments.rs
- crates/aegis-client/tests/shared_steps.rs
- crates/aegis-client/tests/table_conventions.rs
- crates/aegis-client/tests/table_features.rs
- crates/aegis-workbench/src/app/table_chrome.rs
- crates/aegis-workbench/src/app/table_edit.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/tests/table_view.rs
Done when:
1. Fragment tables, grids, filters and conventions pass — `cargo test --locked -p aegis-client --test shared_fragments --test table_conventions --test table_features --test explorer --test grid_edit` (pass: 20 + 9 + 6 + 9 + 27 = 71 passed, 0 failed).
2. Workbench table wiring passes — `cargo test --locked -p aegis-workbench --test table_view` (pass: 10 passed, 0 failed).
3. Full workspace CI passes — `scripts/ci.sh` (pass: workspace fmt, clippy, and tests across all 9 crates).
4. Final persistence/workflow/compatibility regressions pass — `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace` (pass) and `cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded --test relations --test engine_compatibility` (pass).
Notes:
- Counted source lines changed vs base `4b73f3764ca87c216443c05618f78fe397eae406`: +204/-41 (245 changed, cap 520).
- `TableId::Fragments` (`ALL` length 10) added to `Page::SharedBlocks.tables()` (`[SharedSteps, Fragments]`); layout keeps using `SharedSteps` columns and page order/index is unchanged.
- `Folder::Fragments` added under project `SharedAnalysis` only (not `OWNER_TREE`); `Folder::Trees` filters Threat-role trees only; `table_rows_for_owner` returns empty for `Owner::System` and `Owner::Unassigned` on `TableId::Fragments`.
- `table_source_json` for `TableId::Fragments` collects actual `trees/tt*.json` files from `to_workspace_files()` into a presentation-only JSON array in deterministic file order without creating `shared/fragments.json`.
- `target_matches_table` validates `TreeRole::Fragment` for `(TableId::Fragments, Selection::Tree(_))`, rejecting Threat-role trees across `cell_editor`, `edit_cell`, `edit_cells`, `fill_down`, and `paste_tsv`.
- No canvas presentation/actions claimed; GPUI job187 owns fragment canvas acceptance.
NEXT: reviewer
