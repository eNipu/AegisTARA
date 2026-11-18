VERDICT: PASS
## For the owner
What changed: every workbench command now has one GPUI action, and its key and menu entry are built from the command table (keys.rs is gone).
Every key and menu item goes through one `Workbench::run(id)`. That adds the dock toggles, focus/filter explorer, close/next/previous tab, theme toggle, reset layout and quit.
The explorer has a filter box above the tree, and typing in it narrows the rows.
Look at:
- crates/aegis-workbench/src/app/commands.rs:17-61 — one macro writes the action list, `dispatch_id` and the handlers together, so a command id cannot be paired with the wrong action. The test checks that all 22 table ids are in it.
- crates/aegis-workbench/src/app/workbench.rs:460-515 (`run`) — "palette" and "show-shortcuts" fall into `_ => {}` until 182e2. Cmd-K, Cmd-/ and the menu items "Show shortcuts" (AegisTARA menu) and "Palette" (Navigate menu) show up but do nothing for now. This is by plan.
- crates/aegis-workbench/src/app/commands.rs:101-121 — the menus are a fixed list of ids, not grouped by the table's group. "filter-explorer" (Navigate group) is under View, and "palette" (General group) is the only item under Navigate. The menu titles "Edit", "View" and "Navigate" are English `CommandGroup::name()`, not translated. Only the item labels go through the catalog.
Risk: there is no automated test for the new commands' behaviour (dock toggle, filter narrowing, tab step and close, theme, reset layout). The tests prove only that each command has an icon and an action and that the binding count matches the table. I checked the `run` arms by reading them. `step_tab` cannot divide by zero, because `center_tabs` skips empty groups. `close_tab` keeps the tab entry, so reopening it re-adds the panel through `show_tab`. Done-when item 5 (owner check) is NOT done: the builder could not drive a window. The owner must run `cargo run -p aegis-workbench` and confirm three things: the menu bar shows AegisTARA, Edit, View and Navigate; Cmd-B, Cmd-Alt-B and Cmd-J hide and show the docks; typing in the filter box narrows the tree. No new dependency, and no untrusted input beyond local typing.
View: git -C factory/worktrees/182-workbench-skeleton diff 52ecdb0934317a6b706058d31392be2a7228b5d1..HEAD
## Findings
- none blocking. The build used `git reset --soft` instead of `--hard`. The result matches what was intended: HEAD~1 is 52ecdb0, the whole files equal factory/182e-full (gate item 3), and palette_view.rs is not in the commit.
## Lesson
[planner] When a slice adds UI commands, add a done-when item for a gpui key-press test for at least one new command (for example, toggle-explorer closes the left dock), so its behaviour is not proven only by the owner's manual check.
NEXT: human
