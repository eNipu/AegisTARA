Commit: 186039c Add TableTab rating editor popovers for SingleChoice, MultiChoice, and atomic ImpactRatings (189e1b)
Files: crates/aegis-workbench/src/app/mod.rs, crates/aegis-workbench/src/app/table_edit.rs, crates/aegis-workbench/src/app/table_popovers.rs, crates/aegis-workbench/tests/table_edit_view.rs
Done when:
1. Typed rating editor popover tests (`SingleChoice`, `MultiChoice`, and atomic single-undo `ImpactRatings`) pass — `cargo test --locked -p aegis-workbench --test table_edit_view` passed (9 passed; 0 failed).
2. Workbench CI passes clean — `scripts/ci.sh crates/aegis-workbench` passed (`fmt`, `clippy`, `test` all `ok`).
Notes: Changed source lines excluding tests total 342 insertions (6 deletions), under the 380-line cap (`1` in `mod.rs`, `36/6` in `table_edit.rs`, `305` in `table_popovers.rs`). Removed the temporary `#[allow(dead_code)]` on `ActiveEditor` in `table_edit.rs` and focused `self.focus_handle` when opening popover editors so Escape closes them reliably.
NEXT: reviewer
