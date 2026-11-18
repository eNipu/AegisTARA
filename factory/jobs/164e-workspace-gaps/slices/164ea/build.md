# Build 164ea

Commit: ca481e3 Limit journal paths to the file set and treat the open folder as itself (164ea)

Files:
- crates/aegis-model/src/store/workspace.rs (`WorkspaceFile::is_layout_path`, used by `take_files`)
- crates/aegis-model/tests/workspace.rs (`layout_paths_are_exactly_the_written_ones`)
- crates/aegis-client/src/folder.rs (`read_journal` requires layout paths; `WorkspaceFolder::holds`)
- crates/aegis-client/tests/folder.rs (`holds_names_only_this_folder`)
- crates/aegis-client/tests/folder_recovery.rs (4 more hostile paths; `a_journal_outside_the_file_set_touches_nothing`; `a_journal_through_a_link_is_refused` adjusted, see Notes)
- crates/aegis-workbench/src/app/files.rs (`open_folder` no-op, `save_as` becomes Save for the open folder)
- crates/aegis-workbench/tests/folder_view.rs (`opening_the_open_folder_does_nothing`, `save_as_into_the_open_folder_saves`)
- docs/adr/0071-workspace-folder-on-disk.md (one sentence in Recovery on open)

Done when:
1. `cargo test --locked -p aegis-model --test workspace`: ok, 19 passed
2. `cargo test --locked -p aegis-client --test folder_recovery`: ok, 7 passed
3. `cargo test --locked -p aegis-client --test folder`: ok, 15 passed
4. `cargo test --locked -p aegis-workbench --test folder_view`: ok, 8 passed
5. Guard removal tried, each reverted afterwards:
   - `is_layout_path` without `lookup(path).is_some()`: `layout_paths_are_exactly_the_written_ones` FAILED
   - `read_journal` back to `valid_journal_path` only: `a_journal_outside_the_file_set_touches_nothing` and `a_hostile_journal_is_refused_and_touches_nothing` FAILED
   - `holds` as plain `path == self.root` (no canonicalize): `holds_names_only_this_folder` FAILED
   - both `holds` checks removed from `files.rs`: `opening_the_open_folder_does_nothing` and `save_as_into_the_open_folder_saves` FAILED
- `cargo fmt --all` and `scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench`: all steps passed

Notes:
- Changed source lines (no tests/docs): 44 added, 8 removed, under the cap of 130.
- I had to change the existing `a_journal_through_a_link_is_refused`. It used `systems/link/victim.json`, which is not a layout path, so the new check refused it with `Journal` before the link check could give `Symlink`. The test now swaps `systems/main/trees` for a link to the outside folder and names `systems/main/trees/victim.json`, a layout path. It still expects `Symlink { path: "systems/main/trees" }`, so it still covers the link check. The file is on the allowed list.
- The test's check for "an element kind file outside the fixed list" hardcodes the fixed `systems/main` names (item, assets, threats), because `LAYOUT` is private.
- `holds` returns false when the path does not exist, so Save As into a new folder still goes through `create`.

NEXT: reviewer
