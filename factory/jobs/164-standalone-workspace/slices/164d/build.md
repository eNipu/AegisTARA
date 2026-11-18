Commit: 34ed463 Open, save and save as a project folder from the workbench (164d)
Files:
- crates/aegis-client/src/command_table.rs (CommandGroup::File; open-folder, save, save-as rows after palette; 25 rows)
- crates/aegis-client/tests/command_table.rs (count 25)
- crates/aegis-client/locales/{en,ja,pseudo}/main.ftl (cmd-open-folder, cmd-save, cmd-save-as + tips; workbench-status-saved, workbench-status-changed, workbench-converted)
- crates/aegis-workbench/src/app/commands.rs (OpenFolder, Save, SaveAs actions; File menu before Edit)
- crates/aegis-workbench/src/app/icons.rs (hard-drive → IconName::HardDrive)
- crates/aegis-workbench/src/app/files.rs (new: folder, project_name, note, has_unsaved_changes, open_folder, save, save_as, before_close, prompt_open, prompt_save_as)
- crates/aegis-workbench/src/app/workbench.rs (new fields folder, saved_seq, name, methodology; status field dropped; overview_lines; run arms; status_text; title bar shows the name)
- crates/aegis-workbench/src/app/mod.rs (mod files; run(folder: Option<PathBuf>))
- crates/aegis-workbench/src/main.rs (passes the first argument)
- crates/aegis-workbench/tests/folder_view.rs (new, 6 tests)

Done when:
1. `cargo test --locked -p aegis-client --test command_table` ok; `cargo test --locked -p aegis-workbench --test command_wiring` ok (both inside the workspace test run, all passed).
2. `cargo test --locked -p aegis-workbench --test folder_view`: 6 passed.
3. `cargo test --locked -p aegis-workbench`: all passed.
4. `scripts/ci.sh`: FAIL. fmt ok, clippy ok, workspace test fails on one test outside this slice's file list:
   `crates/aegis-client/tests/palette.rs::empty_query_no_match_and_limit` (line 229) expects the first three palette
   commands on an empty query to be `undo, redo, toggle-explorer`. Step 1 of the prompt puts `open-folder`, `save`,
   `save-as` (all `in_palette = true`) right after `palette`, so they are now the first three:
   `left: [Command("open-folder"), Command("save"), Command("save-as")]`. `cargo test --workspace --no-fail-fast`
   shows this is the only failing test.

Blocker (why NEXT: planner):
The prompt fixes the row position ("at the top of the table, after palette") and forbids touching any file outside its
list, and `crates/aegis-client/tests/palette.rs` is not on the list. Either fix makes one of those rules break:
- allow editing `crates/aegis-client/tests/palette.rs` and change the expected three to
  `open-folder, save, save-as` (one-line change; matches the "File menu first" intent), or
- move the three rows below `undo`/`redo`/`toggle-explorer` (contradicts step 1).
The rest of the slice is done and committed; a rework round only needs that one change.

Notes:
- `HardDrive` is not spelled out in gpui-kit-assets source (the enum is generated); `icons/hard-drive.svg` is in
  `default-icons.txt` line 50 and the build compiles, so `IconName::HardDrive` exists.
- Fields used by `files.rs` (and `OVERVIEW`, `Body`, `Tab::body`, `remove`, `changed`, `save_layout`,
  `overview_lines`) are `pub(super)` so the sibling module can reach them.
- `open_folder` also clears `picked` and resets the explorer selection/expansion (keeping the filter text): those keys
  pointed into the old project, and a picked component dropped after the switch would apply to the new one. Small
  addition beyond the prompt.
- `saved_seq` starts as the sample workspace's `confirmed_seq()`; with no folder the status bar keeps the old
  "Sample project in memory" text, with a folder it shows `label  ·  path  ·  Saved|Unsaved changes`.
- `close_saves_unsaved_changes` deviation: the prompt asks for `run("undo")` before the read-only `before_close`.
  To check on disk that the first `before_close` really saved, the test releases the lock and reads `dir` first;
  reopening the folder in the view starts a fresh workspace with no undo history, so the test instead moves the
  component back to the system it came from (the same change the undo makes). It lands in `systems/main/item.json`
  (asserted); `systems/main` and that file are made read-only, `before_close` returns false and `note()` is `Some`;
  permissions are restored before cleanup.
- `an_old_folder_converts_when_opened` uses `old_folder` copied from `tests/convert.rs`, folded with
  `old_folder_keyed` into one function taking the methodology's own key and version.
- Source lines (no tests/locales): about 300 of the 560 cap.
- Owner check (lesson L11, not automated): on macOS, press ⌘O and pick a folder; press ⌘⇧S on the sample and type a
  new folder name; run `cargo run -p aegis-workbench -- <folder>`.

NEXT: planner

## Rework 1
Commit: e5bdf42 Expect the File commands first in the empty palette search (164d)
Files:
- crates/aegis-client/tests/palette.rs (`empty_query_no_match_and_limit`: expected first three targets are now
  `open-folder`, `save`, `save-as`; first key `⌘O`; nothing else changed)

Done when:
1. `cargo test --locked -p aegis-client --test command_table`: 5 passed; `cargo test --locked -p aegis-workbench --test command_wiring`: 2 passed.
2. `cargo test --locked -p aegis-workbench --test folder_view`: 6 passed.
3. `cargo test --locked -p aegis-workbench`: passed (inside `scripts/ci.sh` workspace test).
4. `scripts/ci.sh`: fmt ok, clippy ok, workspace test ok; all steps passed in 9 crates.

Notes:
- Applies prompt step 8a (planner decision). `cargo fmt --all` changed nothing. Everything else from the first
  commit (34ed463) is unchanged; its Notes still hold, including the owner check (⌘O, ⌘⇧S on the sample,
  `cargo run -p aegis-workbench -- <folder>`).

NEXT: reviewer
