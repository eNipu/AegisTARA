VERDICT: PASS
## For the owner
What changed: The overview page now shows only what the chosen system view shows: the counts of every kind, both risk charts, the error and warning counts, the five highest risks and the systems table.
An element that belongs to no system still shows in every view. A damage scenario or control shared by two systems shows for each of them but is counted once.
The problem-to-system lookup was moved into `selection_systems` so the Problems page and the overview use the same rule. The report keeps calling the unfiltered `overview(ws)`.
Look at: crates/aegis-client/src/pages.rs:2893-3004 — errors and warnings are now counted from `problems(ws)` rows, not from lint severity. `problems` makes one row per lint finding with `error = severity == Error` (pages.rs:2765, 2774), so the all-view numbers are the same as before. It does build the row text, which costs a little more.
crates/aegis-client/src/pages.rs:2906 — the five highest risks are taken from the whole project's risk rows and then cut down to the view. A risk number is never worked out again inside a view.
crates/aegis-client/tests/combined_properties.rs:202 — the new property test. Read the note under Risk.
Risk: Low. When the view is all, or names only removed systems, `filtered` is false and every count uses the old totals. `all_view_is_the_whole_project` and `a_removed_system_behaves_as_all` test this.
The first two checks in the property test (filtered rows and top rows equal the unfiltered rows) cannot fail today. `filter_page` and `overview_in` only drop rows from `risk_rows(ws)` and never compute a risk again. The checks are a guard against a future change that does. The third check (a threat in the view is never missing) can fail and is a real test. This is how the prompt asked for it, not a builder defect.
View: git -C factory/worktrees/120-system-overview diff 2d3cc15ebb55a798e2eef516e24aea39bede796d..HEAD
## Findings
- none. Every "Done when" item is met and tested. `every_part_of_the_overview_follows_the_view` checks each part by name against the page rows for `only(brake)` and would fail if the overview were not filtered (it asserts the counts differ from the all view, and checks a rated count of 1 vs 0 for Brake vs ADAS). There is no reachable panic: `rows.len() - errors` cannot underflow. No new dependency, no untrusted input, and the scope matches the prompt.
## Lesson
[planner] A property that compares a filtered subset with the list it was cut from is true by construction. To prove "the same in every view", compute the value a second way (for example on a project rebuilt from only the view's elements, or with a cache cleared) and compare.
NEXT: human
