VERDICT: PASS
## For the owner
What changed: a new license file check (signature, dates, 64 KiB size cap) tells whether editing is allowed; an expired license is still accepted, read-only.
An open server project now carries the version it downloaded (`snapshot`, shared with `Arc`, copied once per download).
The GPUI workbench gets a read-only mode that blocks moves, undo/redo, Save and Save As, and Open Folder now needs a checked license.
Look at:
- crates/aegis-workbench/src/app/files.rs:138-143 — after a folder opens, an expired license makes it read-only. This is the business rule "expired still reads". Check that the wording is what customers should see.
- crates/aegis-workbench/src/app/files.rs:63-75 (`show_project`) — it replaces the shown project without saving. If a folder with unsaved changes is open, those changes are dropped without a question. No caller exists yet; the slice that calls it must run the leave/save step first.
- crates/aegis-client/src/license_file.rs:52-58 — the mapping from license state to editing/grace/days_left.
Risk: low. The only edit path in the workbench is `local::apply_edit` (drop_on) plus undo/redo, and all three are guarded and tested. Save and Save As are refused and tested (the temp folder is checked not to exist). License errors are cut to 200 characters and file reads are capped at 64 KiB + 1, tested with a valid license padded past the cap. One gap stays open: `show_project` drops unsaved folder changes (see above). A later slice must handle it.
View: git -C factory/worktrees/208-gpui-sign-in diff 3b6e4bad1224c8d7bc14f4c69401af04abf7c676..HEAD
## Findings
- none blocking.
- Note: crates/aegis-workbench/tests/read_only.rs:124 — the `redo` step cannot fail on its own, because undo was refused and nothing is left to redo. The undo step and the second move do catch a missing guard, so the item is still proven.
- Note: crates/aegis-workbench/src/app/files.rs:129 — opening the folder that is already open returns before the read-only rule runs. A license that changes while that folder stays open does not update `read_only`. No current flow does this.
## Lesson
[planner] A "show this other project" entry point (`show_project`) replaces the workspace without the unsaved-changes step; when a prompt adds one, say whether it saves or asks first, or the next caller loses edits.
NEXT: human
