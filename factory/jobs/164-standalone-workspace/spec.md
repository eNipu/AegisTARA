# Spec — 164-standalone-workspace

## Title
Open and save a workspace folder in the standalone workbench (WSP-04; D32)

## Design Reference
- `docs/design/workbench-breakdown.md` group 1, row 164 ("Standalone mode opens a folder through the file set; save writes changed files; old project folders convert on open". Done when: "The sample project is edited, saved and reopened with equal numbers; a killed save leaves the old files whole").
- `docs/decisions.md` D32 (a workspace is a folder of JSON files, git is the history), D40 and D43 (files are storage only, every change goes through the model), D42 (one methodology per workspace).
- `factory/jobs/162-workspace-files/spec.md`: the pure file set this job writes to disk (`Project::to_workspace_files`, `Project::from_workspace_files`, `crates/aegis-model/src/store/workspace.rs`, caps `MAX_WORKSPACE_FILES`, `MAX_WORKSPACE_FILE_BYTES`, `MAX_PROJECT_FILE_BYTES`).
- `factory/jobs/182-workbench-skeleton/`: the GPUI workbench (`crates/aegis-workbench`) that today shows only the in-memory sample.
- `factory/jobs/207-gpui-spec/spec.md` section 3: "Workspace folder open and save through the file set — missing — 164"; job order puts 164 before the sign-in and project list (208).
- The old standalone folder format: `crates/aegis-server/src/store/folder.rs` (`FolderStore`: `manifest.json`, `ops.jsonl`, `ops.meta`, `snapshots/`, `figures/`, `.aegis.lock`), rebuilt by `aegis_server::actor::Loaded::rebuild`.
- `docs/engineering-standards.md` section 11 (dialogs only for OS file pickers and irreversible actions).
- Affected paths: new `crates/aegis-client/src/folder.rs`; new `crates/aegis-workbench/src/convert.rs`; `crates/aegis-workbench/src/app/*`; `crates/aegis-client/src/command_table.rs`; `crates/aegis-client/locales/*`; `crates/aegis-workbench/Cargo.toml`; one new ADR; `docs/architecture.md`.

## Problem & Goal
Job 162 can turn a project into a file set and back, but nothing writes it to disk. The GPUI workbench opens only a sample in memory and loses every change on close. Old standalone folders (job 107) use a server-style op log store that D32 retires.

After this job the workbench opens a workspace folder, saves it with ⌘S (only the files that changed, so a git diff stays small), saves the sample or any project into a new folder (Save As), and converts an old project folder the first time it is opened.

Business value: this is the first time a consultant can do real work in the new workbench and keep it. It is also the base that outside-change detection (165), git versions (203), the project list (208) and every later GPUI screen build on. Each moving part earns its place: the save journal is what makes a crash during save harmless (a half-written project could no longer be opened); the conversion keeps existing customer folders usable without a manual export step.

## Acceptance Criteria

1. **Folder API.** `aegis-client` has a module `folder` with `WorkspaceFolder` and `FolderError`:
   - `WorkspaceFolder::open(root, methodology) -> Result<(WorkspaceFolder, Project), FolderError>`
   - `WorkspaceFolder::create(root, &Project)`: the folder does not exist or holds only names that start with `.`; refused otherwise.
   - `WorkspaceFolder::convert_beside(root, &Project)`: writes a full file set into a folder that has no `aegis.json`, leaving every other file untouched.
   - `save(&mut self, &Project) -> Result<SaveReport, FolderError>` where `SaveReport { written: Vec<String>, removed: Vec<String> }`, sorted.
   - Check: `cargo test --locked -p aegis-client --test folder`.
2. **Equal numbers.** The client sample (`aegis_client::sample::sample_ops`) is saved to a new folder, edited (an asset renamed, an attack tree removed), saved, and opened again: the reopened project's `save()` bytes equal the edited project's, and `aegis_client::pages::risk_rows` gives equal rows. The workbench test (criterion 9) repeats this with a component moved between systems. Check: `folder` test.
3. **Only changed files.** Renaming one asset writes exactly `systems/main/assets.json`; a file whose bytes did not change is not rewritten (a test plants different-but-valid bytes in an unchanged file on disk and finds them still there after the save); a removed attack tree's file is deleted. Check: `folder` test.
4. **Refusals, each with a typed error naming the path, and nothing in memory changed:** the folder is open in another `WorkspaceFolder` (OS lock on `.aegis.lock`); `create` into a non-empty folder; a symbolic link anywhere in the read set; a file over `MAX_WORKSPACE_FILE_BYTES`; more than `MAX_WORKSPACE_FILES` files; total bytes over `MAX_PROJECT_FILE_BYTES`; a stray file under `systems/` (the model's `WorkspacePath` refusal); broken JSON. Check: `folder` test.
5. **A killed save leaves whole files.** Save stages every changed file as `<path>.aegis-new`, writes a journal `.aegis-save.json` atomically, renames, then deletes the journal. A save stopped before the journal opens as the old project with no `.aegis-new` left; a save stopped after the journal opens as the new project with no journal left. Check: `cargo test --locked -p aegis-client --test folder_recovery`.
6. **Hostile journal.** A journal naming `../x`, an absolute path, a backslash, an upper-case letter or a path through a symbolic link is refused on open with a typed error, and no file outside the folder is touched. Check: `folder_recovery` test.
7. **Old folders convert on open.** A folder written by `FolderStore` (snapshot plus later ops) opened through `aegis_workbench::convert::convert_legacy` gives the same project as the snapshot with the ops applied in memory; the old files are byte-identical afterwards; the next open reads the file set. A folder with another methodology or version is refused with both names in the message. Check: `cargo test --locked -p aegis-workbench --test convert`.
8. **Workbench commands.** The command table gains `open-folder` (⌘O), `save` (⌘S) and `save-as` (⌘⇧S) with icons, labels and tooltips in English, Japanese and pseudo-locale, a File menu, and handlers. Check: `cargo test --locked -p aegis-client --test command_table`, `cargo test --locked -p aegis-workbench --test command_wiring`.
9. **Workbench behaviour** (GPUI test context): Save As writes the folder and the explorer root shows the folder name; an edit sets "unsaved changes" in the status bar and Save clears it; opening a folder replaces the project and closes element tabs; a refused folder leaves the shown project as it was and shows the error in the status bar; closing the window with unsaved changes in a folder saves first. Check: `cargo test --locked -p aegis-workbench --test folder_view`.
10. **Owner check** (lesson L11, not by an agent): on macOS, ⌘O shows the folder picker and opens a folder; ⌘⇧S on the sample asks for a new folder name and creates it; `aegis-workbench <folder>` from the shell opens that folder.
11. **Written down.** An ADR (next free number) records the save journal, the read rules, the lock, the conversion beside the old files and the decisions below; `docs/architecture.md` lists `folder.rs` and `convert.rs`.
12. **Gate.** `scripts/ci.sh` passes for the whole workspace; `scripts/deny.sh licenses bans sources` passes; no third-party crate new to `Cargo.lock`.

## Decisions
- **Disk code lives in `aegis-client/src/folder.rs`, std only.** The model stays free of disk access (162); the client crate already does file I/O (settings, reports) and every front end can reuse it.
- **Save is a roll-forward journal, not one atomic write per file.** Per-file atomic writes leave a mix of old and new files after a crash, which the strict reader may refuse (a reference to an element in no file); the journal makes the set all-old or all-new.
- **Read set:** root files ending in `.json` except `manifest.json` (the old store's), plus every regular file under `shared/` and `systems/` (depth at most 6); names starting with `.` and names ending in `.aegis-new` are skipped; anything else (`.git`, `README.md`, old store files) is ignored. A stray file under `shared/` or `systems/` is passed on and refused by the model, so nothing is silently dropped. Symbolic links are refused, so a folder cannot read or write outside itself.
- **Lock:** an OS lock on `.aegis.lock` for as long as the `WorkspaceFolder` lives, the same file the old store used, so the Slint shell and the workbench cannot both write one folder at once.
- **Stale files:** save removes only layout files it read or wrote earlier and that the project no longer produces; it never deletes anything else. Empty folders are left.
- **Conversion writes the file set beside the old files and deletes nothing.** The old `ops.jsonl`, baselines and figures stay in the folder (D32 removes history and baselines from standalone; figures have no place in the file set yet). Deleting user data is not worth the risk.
- **Conversion reuses `aegis-server`'s `FolderStore` and `Loaded::rebuild`** instead of a second reader of the old format; `aegis-workbench` gains a dependency on `aegis-server` (already in the workspace) and `tokio` for a current-thread runtime. A later job can slim this when Slint is retired.
- **The project name is the folder name.** The file set has no name field and changing the format is out of scope; the old store's project name is not carried over.
- **Methodology:** the workbench opens folders with the built-in ISO/SAE 21434 + R155 pack; a folder with another methodology is refused by the model (D42). Hosted packs come with the project list (208).
- **Explicit save.** ⌘S saves; Save on the sample (no folder) acts as Save As. Closing the window or quitting with unsaved changes in a folder saves first, without a dialog (section 11 allows dialogs only for irreversible actions; git history is the safety net); if that save fails the window stays open and shows the error. Unsaved changes in the sample are still lost on close, as today.
- **Unsaved changes** means `Workspace::confirmed_seq()` differs from its value at the last open or save; undoing back to the saved state still counts as unsaved, and the next save writes nothing.
- **Start-up:** `aegis-workbench <folder>` opens that folder; with no argument the sample opens. A recent-folders list belongs to the project list (208).
- **Slint shell is unchanged.** It keeps opening old folders through `FolderStore`; edits it makes to a folder after conversion are not seen by the workbench. Both shells coexist only until 192.
- **File commands lead the palette.** 164d keeps open-folder, save and save-as at the top of the command table (File menu first, as in other editors), so the palette's empty-query order changes; `crates/aegis-client/tests/palette.rs` joins 164d's allow list for that one expectation (first three commands and `⌘O`).
- **Directory sync** (`File::open(dir).sync_all()`) runs on Unix only; on Windows renames are not followed by a directory sync.

## Out of scope
- Noticing changes made to the files outside the app (165), bundles (166), git commits, tags and versions (168, 203).
- The project list, recent folders, license check and sign-in in GPUI (208); new workspace from a template (204).
- Writing figures, history or baselines into the file set; any change to the file set layout, the snapshot or the op log.
- Deleting or moving old store files; converting a folder back.
- A disk read benchmark (162's benchmark covers decoding).
- Any change to `aegis-shell` or `aegis-server` sources.
