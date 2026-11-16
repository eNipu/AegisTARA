VERDICT: PASS
## For the owner
What changed: the overview data now carries a systems table, with one row per system (components, assets, threats, highest risk after controls, boundary flows) and a "No system" row, plus a total row.
The table follows the system view. Systems outside the view get no row, and the total is the sum of the rows that are shown.
Tests cover a three-system example and a 64-case random property test. The random test rebuilds each row's highest risk from the uncached model and compares it with the table.
Look at:
- crates/aegis-client/src/combined.rs:246-251. "No system" is dropped when nothing is in it. This matches the struct doc in the prompt, but the screen must not expect that row to be there every time.
- crates/aegis-client/src/combined.rs:205-209. A threat whose risk cannot be evaluated (for example one with no rating yet) counts as unrated and adds no risk level. This is the same as `threat_risks` and the overview risk distribution, so the pages agree.
- crates/aegis-client/src/combined.rs:215-218. In a subset view, a flow that crosses into a hidden system is counted once, at its visible end. So the "twice the total" rule only holds for the "All" view, which is the only view the test checks it on.
Risk: low. The total is built as the sum of the rows, so the rows always add up to it. The real check is that, with the "All" view, the total equals the project's own counts. Both the example test and the random test check that. Levels are compared by their position in the methodology's risk scale, not by colour or label.
View: git -C factory/worktrees/120-system-overview diff db8b3ff236a8e6f1edfabf8bcce7b2f29c9cd3cb..HEAD
## Findings
- none blocking.
- crates/aegis-client/tests/combined_properties.rs:178 (note only). The check against the project's counts uses the view before it is normalized. When the mask selects every system, the check is skipped. With mask 0 it still runs often enough, so this is not a defect.
- crates/aegis-client/tests/combined.rs:618 (`let _ = (fx.adas, fx.gateway);`). This line only silences unused fields. Harmless.
## Lesson
[reviewer] When a "total" is computed as the sum of its rows, the "rows add up" test proves nothing by itself; look for the independent check (total equals the project's own counts) and confirm it runs.
NEXT: human
