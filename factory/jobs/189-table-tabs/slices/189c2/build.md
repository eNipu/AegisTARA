Commit: 27cc2939f5b32fcf4cfa0f2adb3aebf7e229ae33 Wire relation-backed InspectorPanel into Workbench (189c2)
Files:
- crates/aegis-workbench/src/app/panels.rs
- crates/aegis-workbench/src/app/inspector.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/src/app/files.rs
- crates/aegis-workbench/src/app/mod.rs
- crates/aegis-workbench/tests/picker_view.rs

Done when:
1. Multi-link picker and inspector headless GPUI tests pass (live toggle without double-lease, undo, stale-selection clearing, read-only guard) — `cargo test --locked -p aegis-workbench --test picker_view`: PASS (9 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench`: PASS (fmt, clippy, test all ok)

Notes:
- Counted non-test source lines from `git diff --numstat HEAD~1`: 454 insertions / 19 deletions (473 total), under the 480-line cap without `#[rustfmt::skip]` or compression.
- `Workbench::open_target_skipping` selects the explorer row directly (`self.state.select(key); self.show_rows(cx); self.open(...)`) and then calls `self.select_element_skipping(Some(sel), skip, cx)` rather than going through `self.select(key, cx)` (which has `skip = None`), preventing GPUI double-lease panics when clicking link chips inside `InspectorPanel` or its inline `MultiLinkPickerView`.
- `InspectorPanel::open_chip` updates the inspector's own state from `wb.read(cx)` right after `w.open_target_skipping(target, Some(my_id), window, cx)` so clicking a chip in the inspector also transitions the inspector panel to the clicked element without double-leasing.
- `Workbench::kind` was broadened to `pub(super)` for reuse in `InspectorPanel`.
- Added 4 headless GPUI tests in `tests/picker_view.rs` covering `Threat`/`Goal` relation rows against `RELATIONS` (plus excluded-badge rendering and non-editable relation refusal), live keyboard toggle inside the docked inspector without double-lease or picker reopening, `Workbench::run("undo")` byte/chip/picker restoration, chip navigation & selection-switch picker close, deleted-element and `show_project` (`replace`) clearing, and read-only refusal with positive control when read-only is cleared.

## Rework 1

Commit: 9dc3c6acb9d65aea44a5ca2b864716f010f26032 Test inspector Escape close and same-UUID project replace (189c2)
Files:
- crates/aegis-workbench/tests/picker_view.rs

Done when:
1. Multi-link picker and inspector headless GPUI tests pass (live toggle without double-lease, undo, stale-selection clearing, read-only guard) — `cargo test --locked -p aegis-workbench --test picker_view`: PASS (9 passed, 0 failed)
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench`: PASS (fmt, clippy, test all ok)

Notes:
- Fixed review finding 1 (`tests/picker_view.rs` `inspector_clears_on_deleted_selection_and_project_replace`): cloned the active workbench project (`vcx.read_entity(&wb, |w, _| w.workspace().project().clone())`) and asserted `same_ids.asset_idx(&asset).is_some()` before calling `show_project`, so only `Workbench::replace`'s explicit `self.select_element(None, cx)` clears the inspector (not stale-ID detection).
- Fixed review finding 2 (`tests/picker_view.rs` `inspector_inline_picker_toggles_without_double_lease_undoes_and_closes_on_reselect`): opened the inline picker in the docked inspector, drew it, sent `"escape"` via `simulate_keystrokes("escape")`, and asserted `inspector.active_picker().is_none()` while `inspector.selection()` stays intact and redraws cleanly, directly exercising `set_on_close` (`me.update`) from `MultiLinkPickerView::handle_key("escape")`.
- Counted non-test source lines vs `3173ca9fc7173a903e58cb146afc03d8e2946512`: unchanged at 454 insertions / 19 deletions (473 total, under 480 cap).

NEXT: reviewer
