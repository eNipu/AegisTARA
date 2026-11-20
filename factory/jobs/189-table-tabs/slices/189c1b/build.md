Commit: 3173ca9 Add reusable keyboard-operable GPUI MultiLinkPickerView (189c1b)
Files:
- crates/aegis-workbench/src/app/picker_view.rs
- crates/aegis-workbench/src/app/mod.rs
- crates/aegis-workbench/tests/picker_view.rs

Done when:
1. Multi-link picker headless GPUI tests pass (keyboard toggle, live refresh without reopening, undo, read-only refusal) — `cargo test --locked -p aegis-workbench --test picker_view`: PASS (5 passed, 0 failed).
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench`: PASS (fmt, clippy, test all ok).

Notes:
- Implemented `MultiLinkPickerView` in `crates/aegis-workbench/src/app/picker_view.rs` and re-exported it from `crates/aegis-workbench/src/app/mod.rs`.
- Uses `capture_key_down` so `"up"`, `"down"`, `"enter"`, and `"escape"` are intercepted before the focused filter `Input`, while leaving `"space"` (when the filter input is focused), normal typing, and modifier shortcuts (such as `cmd-z`) untouched. When the picker container itself is focused (filter unfocused), `"space"` toggles the active option.
- All edits and chip target jumps go through `Workbench::apply_edit_skipping` and `Workbench::open_target_skipping` passing `Some(cx.entity_id())`, preventing synchronous GPUI double-lease panics; immediately after `apply_edit_skipping`, `MultiLinkPickerView` refreshes its `MultiLinkPicker` from `wb.read(cx).workspace().project()` so linked chips and option flags stay live while keeping the picker open and filter intact.
- Bounded option rendering slices `picker.window(10)` and computes each row's true visible index via `visible.binary_search(first)` so cursor highlighting and row clicks remain accurate when scrolled past 10 candidates.
- Changed source lines (excluding `tests/`): 375 lines (`373` in `picker_view.rs` + `2` in `mod.rs`), within the 380-line cap without `#[rustfmt::skip]`.

NEXT: reviewer
