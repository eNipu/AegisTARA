VERDICT: PASS
## For the owner
What changed: when you pick a system view, table rows that belong only to other systems are hidden. This covers the Item, Assets, Damage, Risks, Trees, Controls, Goals, Problems and Threats pages.
Rows that belong to no system always stay. Switching the view only changes what the screen shows: it sends nothing to the server and adds no undo step, in both edit and read-only mode.
The project view also carries the view menu (the chip) and the palette entries that switch views, ready for the GPUI screen. The rework changed only the test file.
Look at: crates/aegis-client/src/session.rs:3371 (`shown_threats`): the Threats list, the selected threat and "select row N" all use the filtered list, so a hidden threat cannot stay selected.
crates/aegis-client/src/systems.rs:299 (`filter_page`): one rule per page. A row whose ID is no longer in the project is kept, not hidden.
crates/aegis-client/tests/systems_view.rs:444: checks the row count of each kind for the "all" view and the Brake view. It now covers controls, goals, claims, problems and the Threats list (`filter_threats` is called directly).
Risk: low. The filter only removes rows from lists built for the screen and never touches the project. The tests show the workspace, pending batches and undo state are unchanged after filtering. The session test shows switching adds no op, in edit and read-only mode. Two things are still untested, and both are simple one-line rules: the Item page's data flows and trust boundaries (the test project has none), and a Problems row whose target is a tree node.
View: git -C factory/worktrees/119-systems diff 11736f05491b35e1e00fd1e6b29227d56cec855d..HEAD
## Findings
none
## Lesson
none
NEXT: human
