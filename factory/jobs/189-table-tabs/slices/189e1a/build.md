Commit: 32133b1 Add TableTab keyboard navigation, inline Text and LinkPicker editors, and multi-cell commit (189e1a)
Files: crates/aegis-workbench/src/app/mod.rs, crates/aegis-workbench/src/app/table_edit.rs, crates/aegis-workbench/src/app/table_tab.rs, crates/aegis-workbench/tests/table_edit_view.rs
Done when:
1. Keyboard navigation, inline Text & LinkPicker editing, multi-cell commit, error preservation, and read-only GPUI tests pass — `cargo test --locked -p aegis-workbench --test table_edit_view` -> passed (6 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench` -> passed (`fmt`, `clippy`, `test`: ok)
Notes:
- Changed source lines excluding tests: 478 added (`1` in `mod.rs`, `443` in `table_edit.rs`, `34` in `table_tab.rs`) via `git diff --cached --numstat`, under the 480-line cap.
- `ActiveEditor` carries `#[allow(dead_code)]` for `SingleChoice`/`MultiChoice`/`ImpactRatings` rendering in `189e1b`, and `TableTab::start_edit_with_project` carries `#[allow(dead_code)]` for `189e2`'s `grid-add-row` caller.
- Capture-phase Escape in `TableTab::handle_key_event` skips `ActiveEditor::LinkPicker` so `MultiLinkPickerView::on_capture_key_down` closes the picker cleanly and triggers its `on_close` callback to clear `editor` and restore `TableTab` focus.
NEXT: reviewer

## Rework 1
Commit: 88ab2f3 Trim table_edit and table_tab line count under gate cap (189e1a)
Files: crates/aegis-workbench/src/app/table_edit.rs
Done when:
1. Keyboard navigation, inline Text & LinkPicker editing, multi-cell commit, error preservation, and read-only GPUI tests pass — `cargo test --locked -p aegis-workbench --test table_edit_view` -> passed (6 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench` -> passed (`fmt`, `clippy`, `test`: ok)
Notes:
- Reduced `crates/aegis-workbench/src/app/table_edit.rs` cleanly without `#[rustfmt::skip]`: removed unused `Rc` import, simplified `sync_editor_on_refresh`, `self_refresh`, `open_editor_kind`, `start_edit_with_project`, `start_edit`, `commit_input`, `handle_grid_key`, and `render_editor_overlay`.
- Total counted lines across `eb90034f333a181db6d7b3b6bf24ab697525aa21..HEAD` (`git diff --numstat`): 473 (`1` in `mod.rs`, `428` in `table_edit.rs`, `34+10=44` in `table_tab.rs`), within the 480-line cap.
NEXT: reviewer
