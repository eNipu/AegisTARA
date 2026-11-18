# Build 182d3

Commit: 52ecdb0 Move components by drag or keys with undo and redo from the command table (182d3)
Files: crates/aegis-workbench/src/app/keys.rs (new), crates/aegis-workbench/src/app/mod.rs, crates/aegis-workbench/src/app/panels.rs, crates/aegis-workbench/src/app/workbench.rs, crates/aegis-workbench/tests/explorer_view.rs. All taken unchanged with `git checkout factory/182d-full -- ...`.

Done when:
1. `cargo test --locked -p aegis-workbench --test explorer_view`: ok, 3 passed (keys_move_the_selection_and_open_a_tab, pick_up_and_drop_moves_a_component_and_undo_restores, a_closed_dock_stays_closed_on_the_next_start).
2. `cargo test --locked -p aegis-workbench`: all pass (explorer_view 3, local 2, sample 2, theme 4, window 1).
3. `git diff --quiet factory/182d-full HEAD`: exit 0, the branch holds exactly the full 182d build.
4. `git diff --numstat HEAD~1 HEAD -- . ':!crates/*/tests'`: the pathspec does not exclude the test file (prints 81/62 for tests/explorer_view.rs). Source only: keys.rs 58/0, mod.rs 3/0, panels.rs 62/6, workbench.rs 151/6 = 274 added, 286 with deletions. Under 340.
5. Owner check (L11): drag a component onto another system, watch the Components counts change, then `⌘Z` restores them. Not done by the builder (needs a display); the GPUI test covers the same path by keys.

Before you commit: `cargo fmt --all` changed nothing; `scripts/ci.sh crates/aegis-workbench` all steps passed (fmt, clippy, test); `cargo test --locked -p aegis-workbench` passed.

Notes (already true of the taken files):
- `app/keys.rs`: `actions!(workbench, [ExplorerDown, ExplorerUp, ExplorerExpand, ExplorerCollapse, OpenSelected, PickUp, DropHere, Undo, Redo])` and `pub fn bind_keys(cx: &mut App)` building every binding from `command_table::commands()` with `keys_for(cfg!(target_os = "macos"))` and the command's context; no shortcut typed elsewhere. `mod.rs` calls `bind_keys(cx)` in `run()` and re-exports it. Slice 182e replaces these hand-listed actions with one action per command.
- `app/workbench.rs`: `Body` (Overview, List, Element) stored per tab so `changed` rebuilds the `ExplorerIndex` and refreshes every open list and element tab after each edit; `drop_on(dragged, target)` runs `explorer::drop_edit` and `local::apply_edit` (actor from `local::actor()`); `on_pick_up` holds only a component element; `on_drop_here` drops the held one on the selected row; `on_undo` / `on_redo` call `local::undo` / `local::redo`; edit errors show in the status bar; while a component is held the status bar shows `workbench-picked` with the `drop-here` shortcut label from the table.
- `app/panels.rs`: component rows are draggable (`on_drag` with their `ExplorerKey`, a `DragLabel` under the pointer); System, "Not in a system" and folder rows accept `on_drop` and call `drop_on`; Escape (`on_key_down`, no command-table row) calls `let_go`; `text_panel!` gains `set`.
- `tests/explorer_view.rs`: command-table keys move the selection and open a tab; pick-up "Telematics unit", select Gateway, drop-here takes Gateway's Components count 1 -> 2 and clears the held one, undo 1, redo 2; the closed-dock test from 182d. The temporary `opening_rows_adds_tabs_by_key` from 182d2 is gone.

NEXT: reviewer
