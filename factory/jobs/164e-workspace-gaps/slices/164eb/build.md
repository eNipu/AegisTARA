# Build 164eb

Commit: 15f650e Ask Save, Don't save or Cancel before unsaved changes would be lost (164eb)

Files:
- crates/aegis-workbench/src/app/files.rs (`Leave`, `request_close`, `leave`, `ask`, `finish`, `open_inner`, `prompt_save_as(then)`; `before_close` removed)
- crates/aegis-workbench/src/app/workbench.rs (fields `asking`, `leaving`; close hook passes `window`; `quit`, `save`, `save-as` wiring)
- crates/aegis-workbench/src/app/mod.rs (`pub use files::Leave`)
- crates/aegis-workbench/tests/folder_view.rs
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (8 `workbench-leave-*` strings each)
- docs/adr/0071-workspace-folder-on-disk.md ("Leaving with unsaved changes")

Done when:
1. `cargo test --locked -p aegis-workbench --test folder_view`: 14 passed. New tests: close_saves_a_folder_without_asking, a_failing_save_asks_and_dont_save_closes (unix), sample_edits_ask_before_close, sample_without_edits_closes_without_asking, opening_a_folder_over_sample_edits_asks, a_refused_folder_after_dont_save_keeps_the_edits, quit_with_sample_edits_asks.
2. `scripts/ci.sh`: fmt, clippy and test ok, "all steps passed in 9 crate(s)".
3. `cargo test --locked -p aegis-workbench --test command_wiring`: 2 passed.
4. Owner check (L11, not done by me): on macOS, close the window with sample edits. The native dialog should appear, Escape should cancel it, and Don't save should close the window.

Notes:
- `finish` takes a `discard: bool` instead of a separate `finish_discarding` function. It is the same behaviour as the prompt describes, with less code.
- When a folder saves fine, `request_close` calls `window.remove_window()` from inside the should-close hook and returns `false`, as the prompt says. The tests check that the window has left `cx.windows()`. The owner check should also confirm that a plain close of a folder that saves closes the real macOS window.
- close_saves_a_folder_without_asking: after the window closes, the test's `view` handle keeps the Workbench alive, so its folder lock stays held and `WorkspaceFolder::open` returns `Locked`. The test therefore copies the folder, without `.aegis.lock`, and opens the copy to check the moved component.
- a_failing_save_asks_and_dont_save_closes saves the move first, then makes `systems/main` read-only and moves the component back, so the next save must write that folder. This is the same setup as the old test, without the reopen.
- The Open-after-Don't-save refusal is in its own `#[gpui::test]` (a_refused_folder_after_dont_save_keeps_the_edits). It uses the bad `aegis.json` folder from a_refused_folder_keeps_the_project.
- Changed source lines (src only): 155 added, 31 removed. The cap is 360.

NEXT: reviewer
