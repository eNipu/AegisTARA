VERDICT: PASS
## For the owner
What changed: closing the window, quitting or opening another folder no longer silently drops sample edits or gets stuck on a save that keeps failing.
A folder that saves fine is still saved with no dialog. The sample with edits, or a folder whose save failed, now gets one dialog: Save As… (or Try again), Don't save, Cancel.
Eight new dialog strings in English, Japanese and pseudo; a short ADR 0071 section; seven tests replace the old close test.
Look at: crates/aegis-workbench/src/app/files.rs:161 (`request_close` calls `window.remove_window()` from inside the OS should-close hook and returns `false`. The tests only call it directly, so the real macOS close path needs your hands-on check)
crates/aegis-workbench/src/app/files.rs:174 (`leave`: the four rules, plus the `asking` guard that stops a second dialog)
crates/aegis-workbench/src/app/files.rs:228 (the answer handler: 0 means Try again or Save As…, 1 means Don't save, and anything else, including a dropped dialog, means Cancel)
Risk: (1) The real window close goes through GPUI's should-close hook, and no test drives that hook. Your manual check covers it: on macOS close a saved folder (the window should close), then close the sample with edits (dialog appears; Escape cancels; Don't save closes). (2) `asking` covers only the dialog, not the Save As… folder picker that opens after it. If you click close again while that picker is open, a second leave dialog can appear next to it. This does not crash, because the GPUI prompt panic is only about two `window.prompt` calls. The worst case is two dialogs on screen. Every tested row checked: folder saves silently, failed save asks / Try again asks again / Don't save closes / Cancel keeps, sample asks / Save As… saves and closes, open over sample edits asks, refused open after Don't save keeps the edits, quit asks.
View: git -C factory/worktrees/164e-workspace-gaps diff ca481e3e244a06d00f673de3aa23d868207e8531..HEAD
## Findings
- none blocking. Optional follow-up: also keep `asking` set while the picker started by Save As… is open (files.rs `ask`, `Some(0)` arm), so closing again during the picker does nothing.
## Lesson
[reviewer] When a slice drives a dialog or a window close from a test, check whether the test calls the OS hook or the method behind it. Tests that call the method directly leave the hook wiring to the owner check.
NEXT: human
