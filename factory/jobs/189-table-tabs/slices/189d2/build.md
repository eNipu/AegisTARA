Commit: eb90034 Add TableTab column chooser, per-column filters, and developer source view (189d2)
Files:
- crates/aegis-workbench/src/app/mod.rs
- crates/aegis-workbench/src/app/table_chrome.rs
- crates/aegis-workbench/src/app/table_tab.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/tests/table_view.rs

Done when:
1. Column chooser, column filters (including `open_filter_for` switching without clobbering and hidden filtered column), and developer source view tests pass — `cargo test --locked -p aegis-workbench --test table_view` -> ok (10 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench` -> ok (fmt, clippy, test all passed in 1 crate)

Notes:
- Counted source diff lines excluding `tests/`: 518 added, 18 deleted (536 total changed lines, under `max_lines` cap of 560).
- `TableTab::toggle_column` calls `Workbench::set_table_column_visible_skipping(..., Some(cx.entity_id()), cx)` and refreshes `self` directly via `apply_refresh` + `cx.notify()` (since `wb.read(cx)` borrows `cx` immutably while reading `w.workspace()`).
- Added small inspection accessors `TableTab::open_filter_column(&self)` and `TableTab::filter_input(&self)` so integration tests assert real popover state and live `InputEvent::Change` delivery via `InputState::insert`.

NEXT: reviewer
