VERDICT: PASS
## For the owner
What changed: the explorer now acts on the project. Arrow-style keys, open, pick-up, drop-here, undo and redo are bound from the command table (one place for every shortcut).
A component row can be dragged onto a system, "Not in a system" or a folder row. Escape lets go of a picked-up component, and while one is held the status bar names it.
After each edit, undo or redo, the tree counts and the open list and element tabs are rebuilt. An edit error shows in the status bar.
Look at: crates/aegis-workbench/src/app/workbench.rs:372 (`changed`): this refreshes the open tabs. A tab is found by its explorer key, and that key includes the owning system. So after a move, the moved component's own element tab is no longer refreshed. Its text (id, name, kind) does not depend on the system, so nothing wrong shows today. But opening the component again under its new system opens a second tab.
crates/aegis-workbench/src/app/panels.rs:203: drag and drop by mouse. No test covers this path. It calls the same `drop_on` as the keyboard path, which is tested.
crates/aegis-workbench/src/app/workbench.rs:463 (`status_text`): an edit error stays in the status bar until the next edit, undo or redo succeeds. Selecting another row or pressing Escape does not clear it.
Risk: low. The keyboard path is checked by tests/explorer_view.rs. That test picks up "Telematics unit", drops it on Gateway and checks the Components count goes 1 -> 2, that the held component is cleared, that undo gives 1 and redo gives 2. The test fails if the bindings, `drop_on`, `changed` or undo/redo are removed. The mouse drag is not tested: the owner check (L11) is still to do, because the builder had no display. Drag a component onto another system, watch the counts change, then press Cmd+Z.
View: git -C factory/worktrees/182-workbench-skeleton diff cdb8348e9ed0cc16a9c829ca2ddcf3d94469f777..HEAD
## Findings
- None blocking. The prompt requires the branch to equal factory/182d-full byte for byte (Done when 3), so the small points under "Look at" are left for a later slice. They are the stale tab key after a move and the sticky error note.
- Done when 4: the given pathspec `':!crates/*/tests'` does not exclude the test file. The builder counted the source by hand (286 lines, under 340), and the gate agrees (286).
## Lesson
[planner] Center tabs are keyed by an ExplorerKey that includes the owner, so a tab loses its key when its element moves. When a slice adds an edit that moves elements, key tabs by the element (Selection), not by the tree position.
NEXT: human
