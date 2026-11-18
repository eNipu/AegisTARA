# Spec — 164e-workspace-gaps

## Title
Workspace gaps from the 164 reviews: ask before dropping changes, the open folder again, journal paths limited to the file set

## Design Reference
- `factory/backlog.md` line "Workspace gaps from the 164 reviews" (items 1 to 3).
- `factory/jobs/164-standalone-workspace/spec.md` (Decisions "Explicit save", "Lock") and the reviews `factory/jobs/164-standalone-workspace/slices/164b/review.md` (a valid journal can replace or remove any `*.json`, `.git/*.json` included) and `.../164d/review.md` (risks 1 to 3: window cannot close while the save keeps failing; the open folder again gives "Locked"; sample edits dropped without asking).
- `docs/engineering-standards.md` section 11 (a modal dialog only for an action that cannot be undone; main button says the action; destructive button not the Enter default; Escape cancels).
- `docs/adr/0071-workspace-folder-on-disk.md` (save journal, recovery on open).
- Affected paths: `crates/aegis-model/src/store/workspace.rs`, `crates/aegis-client/src/folder.rs`, `crates/aegis-workbench/src/app/files.rs`, `crates/aegis-workbench/src/app/workbench.rs`, `crates/aegis-client/locales/{en,ja,pseudo}/main.ftl`, their tests, ADR 0071.

## Problem & Goal
Job 164 left three gaps the reviews named. (1) If a save keeps failing (folder deleted, disk read-only) the window can never be closed, and edits in the in-memory sample are dropped without a word when the user closes, quits or opens a folder. (2) Opening the folder that is already open, or Save As into it, fails with a confusing "Locked" error. (3) A save journal on disk is trusted to replace or remove any `*.json` in the folder, `.git/` included.

After this job: leaving with changes that would be lost asks once (Save, Don't save, Cancel); the open folder again is a no-op and Save As into it is a plain Save; recovery only ever touches paths of the workspace file set.

Business value: a consultant never loses a sample edit by accident and can always close the app; the error for a harmless action disappears; a planted or corrupted journal cannot damage a git history or other files the customer keeps in the folder. Each part is small and closes a reviewed risk; nothing new is added beyond it (P7).

## Acceptance Criteria
1. **Layout path check.** `aegis_model::WorkspaceFile::is_layout_path(path: &str) -> bool` is true exactly for paths `Project::from_workspace_files` accepts by name: true for every path `to_workspace_files` writes for a project with every element kind and trees; false for `.git/config.json`, `manifest.json`, `notes.json`, `shared/other.json`, `systems/other/assets.json`, `systems/main/trees/a/b.json`, `systems/main/trees/.json`, `../aegis.json`, `Aegis.json`, `""` and a 300-byte path. Check: `cargo test --locked -p aegis-model --test workspace`.
2. **Journal paths limited.** A journal whose `replace` or `remove` names a path that is not a layout path (`.git/config.json`, `manifest.json`, `notes.json`, `systems/main/trees/a/b.json`) is refused on open with `FolderError::Journal`, after a valid entry, and the planted `.git/config.json` and `notes.json` and `shared/damage.json` keep their bytes. Check: `cargo test --locked -p aegis-client --test folder_recovery`.
3. **Same folder test.** `WorkspaceFolder::holds(&self, path: &Path) -> bool` is true for the root, for `root/shared/..` and (Unix) for a symbolic link to the root; false for another folder and a missing path. Check: `cargo test --locked -p aegis-client --test folder`.
4. **Open folder again.** `Workbench::open_folder` on the open folder (any spelling of its path) returns `Ok`, changes nothing (project bytes, unsaved flag, open tabs) and writes nothing. `Workbench::save_as` into the open folder saves it (unsaved becomes false; after the lock is released the folder reopens equal to the edited project). Check: `cargo test --locked -p aegis-workbench --test folder_view`.
5. **Ask before dropping changes.** Close (window close and `request_close`), Quit and Open Folder:
   - no unsaved changes: go on, no dialog;
   - a folder with unsaved changes that saves: saved, go on, no dialog (as in 164);
   - a folder whose save fails: one dialog titled with the project name, the error as its detail, buttons "Try again", "Don't save", "Cancel"; Try again saves again and asks again if it fails;
   - the sample with unsaved changes: one dialog, buttons "Save As…", "Don't save", "Cancel"; Save As… opens the new-folder picker and goes on after a successful Save As;
   - Don't save goes on and drops the changes; Cancel changes nothing; a refused open after Don't save keeps the edits, still unsaved.
   Check: `folder_view` tests with `simulate_prompt_answer` for every row above, Quit with Cancel included.
6. **Strings** for the dialog in English, Japanese and pseudo-locale with hyphenated Fluent ids. Check: `scripts/ci.sh` (the catalog tests).
7. **Written down.** ADR 0071 states the journal path rule and the leave dialog.
8. **Owner check** (lesson L11): on macOS, closing the window with sample edits shows the dialog; Escape cancels; Don't save closes.
9. **Gate.** `scripts/ci.sh` passes for the whole workspace; no new dependency.

## Decisions
- **A folder that saves is still saved silently on leave.** Section 11 allows a dialog only for an action that cannot be undone; a successful save loses nothing and git is the history. The dialog appears only when changes would be lost: the sample, or a failed save.
- **Opening the open folder does nothing** (no save, no reload), the cheapest correct reading of "does nothing".
- **Same folder** is decided by comparing `fs::canonicalize` of both paths; if either fails, they are not the same.
- **Layout check lives in the model** next to `lookup`, so the journal and the reader can never disagree about which names are workspace files.
- **The symlink check-then-use gap stays** (the attacker already has write access), as the backlog says.
- **The dialog uses GPUI's platform prompt** (`Window::prompt`), the native alert on macOS; "Save" or "Try again" is the Ok button, "Don't save" an ordinary button, "Cancel" the Cancel button (Escape).
- **Re-entrancy:** while the dialog is open, another close, quit or open request does nothing (GPUI refuses a second prompt).

## Out of scope
- Noticing outside changes (165), autosave, recent folders (208).
- Any change to the file set layout, the journal format or the lock.
- The Slint shell.
