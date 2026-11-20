Commit: 09da3791e9d694b5a69448e9d9d1e61a308335f6 Wire TableTab row editing, fill down, TSV clipboard, and grid commands (189e2)
Files:
- crates/aegis-workbench/src/app/table_edit.rs
- crates/aegis-workbench/src/app/table_chrome.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/tests/table_edit_view.rs

Done when:
1. All spreadsheet editing GPUI tests pass: `cargo test --locked -p aegis-workbench --test table_edit_view` -> ok (14 passed; 0 failed).
2. Table rendering and picker tests still pass: `cargo test --locked -p aegis-workbench --test table_view && cargo test --locked -p aegis-workbench --test picker_view` -> ok (10 passed; 0 failed and 9 passed; 0 failed).
3. Full workspace CI passes clean: `scripts/ci.sh` -> ok (all steps passed in 9 crates).

Notes:
- Pure prepare helpers (`can_run_grid_command`, `prepare_add_row`, `prepare_delete_selected`, `prepare_fill_down`, `prepare_paste_tsv`, and `focus_added_row_with`) take `&Project` directly and never borrow `Workbench`, so both `TableTab` public methods (using `apply_edit_skipping` + `self_refresh`) and `Workbench::run` (using `apply_edit` + `tab.update`) avoid double-leasing.
- `prepare_delete_selected` and `delete_selected` strictly refuse `TableId::Problems` and `source_mode`, preventing accidental deletion of model elements referenced by lint problem rows.
- Changed source lines (excluding `tests/`): 338 lines (under the 480-line cap).

NEXT: reviewer
