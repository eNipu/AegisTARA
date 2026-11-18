VERDICT: PASS
## For the owner
What changed: The workbench gets a File menu with Open Folder… (⌘O), Save (⌘S) and Save As… (⌘⇧S), in all three languages.
The project now lives in a folder: its name shows as the project name, the status bar says Saved or Unsaved changes, and closing or quitting saves first.
`aegis-workbench <folder>` opens that folder at start-up, and old project folders are converted when opened.
Look at: crates/aegis-workbench/src/app/files.rs:46-86 (`open_folder`: it saves the old folder first, changes nothing on error, then swaps the project and closes the element tabs; it also clears the picked component and the explorer selection, which the prompt did not ask for but which stops a stale pick from landing in the new project)
crates/aegis-workbench/src/app/files.rs:126-140 with workbench.rs:230 (`before_close`: if the save fails, the window refuses to close and the error shows in the status bar)
crates/aegis-workbench/src/app/workbench.rs:553-558 (Save with no folder open falls back to Save As)
Risk: (1) If saving keeps failing (folder deleted, disk read-only), the window cannot be closed or quit until the folder is writable again. The prompt asked for this, and no extra dialog is allowed. The `close_saves_unsaved_changes` test checks it with a read-only folder. (2) Opening the folder that is already open, or Save As into it, fails with "Locked" because the view still holds that folder's lock. Nothing changes and the error shows in the status bar, but the message is confusing. (3) Opening a folder while the in-memory sample has unsaved edits drops those edits without asking, as the prompt says (the sample already warns that its changes are not saved). (4) The OS folder pickers and the command-line argument were not tested automatically; the owner check in build.md covers them.
View: git -C factory/worktrees/164-standalone-workspace diff 7fbdd03ed7c4bdb42d920f3e3ef7df6ab4d3f952..HEAD
## Findings
- none blocking. Every "Done when" item is met (gate.md: ci.sh, command_table, command_wiring, folder_view all ok). The folder_view tests can fail when the code is wrong: they check the files on disk after a release-and-reopen, check that state is unchanged after a refused open, and check that the save really failed on a read-only folder. They do not reopen the folder between edits, so stale state would not be hidden.
- crates/aegis-workbench/tests/folder_view.rs:271 — the test moves the component back instead of using `run("undo")`, because reopening the folder clears the undo history. It still rewrites `systems/main/item.json` and the test asserts that, so it is an accepted deviation.
- The status bar text (`status_text`, workbench.rs:838-850) has no test. It is not a "Done when" item; noted only.
## Lesson
none
NEXT: human
