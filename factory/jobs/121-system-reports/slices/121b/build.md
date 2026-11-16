# Build 121b

Commit: dfd26ff Add a Systems column and the systems filter to the workbook (121b)
Files:
- crates/aegis-client/src/systems.rs (`systems_text`)
- crates/aegis-client/src/report/xlsx.rs (`Rows` helper, `report_workbook_xlsx_in`, `recount`, filtered/columned sheets)
- crates/aegis-client/src/report/mod.rs (re-export)
- crates/aegis-client/src/session.rs (ReportXlsx arm calls `report_workbook_xlsx_in` with the profile's systems)
- crates/aegis-client/tests/system_reports.rs (five new tests)

Done when:
1. `cargo test --locked -p aegis-client --test system_reports`: ok, 11 passed (5 new).
2. `cargo test --locked -p aegis-client --test report_builder --test reduced_by_report --test pages`: ok (22, 14, 14 passed).
3. `cargo check --locked -p aegis-shell`: ok.
Before commit: `cargo fmt --all` ok; `scripts/ci.sh crates/aegis-client` fmt/clippy/test all ok.

Notes:
- A private `Rows` struct in xlsx.rs holds `has_systems`, the `SystemUse` (computed only when the project has systems) and the `Scope`. `Rows::header` appends "Systems" when the project has systems. `Rows::row(row, set)` returns `None` when the row is hidden, and otherwise adds the Systems cell. Sheets call `rows.extend(sys.row(..))`. The twelve element sheets use it. Security settings, attack steps (they loop over the filtered tree rows) and problems use `Rows::shows` only.
- Overview (step 6): the Overview sheet is still built first, as today. When `scope.is_on()`, `recount` sets each count to the data-row count of the sheet whose name is `sheet_name(label)` before `unique_names` runs. The result is the same as building the element sheets first and inserting the Overview at 0, with a smaller diff. When the scope is off, nothing changes.
- `risk_register_xlsx` passes `Rows::new(p, &SystemView::all())`, so it gets the column when the project has systems.
- `report_workbook_xlsx_with_figures` lost its `too_many_lines` allow because it is now a one-line call; the allow moved to `report_workbook_xlsx_in`.
- Line count outside tests (`git diff --numstat`): 262 added and 88 removed. Most of the removals are the same lines rewrapped into `extend(sys.row(vec![..], ..))`. The added lines are under the 300 cap. If the cap means added plus removed, the total is 350.
- `a_project_with_no_systems_exports_as_before` also checks that a stale filter name on a project with no systems gives the same workbook.

## Rework 1

Gate: "too big to review: 350 changed lines, cap 300".

Commit: ad7d39b Shrink the workbook systems change to keep each row builder as it was (121b)
Files: crates/aegis-client/src/report/xlsx.rs

- Replaced `Rows::header` and `Rows::row` with `Rows::head(&mut rows)` (adds the "Systems" header), `Rows::cell(set) -> Option<Vec<String>>` (`None` when the row is hidden, an empty Vec when the project has no systems) and a free `end_row(&mut rows, cell)`. Each sheet loop now starts with `let Some(cell) = sys.cell(..) else { continue; };` and ends with `end_row(..)`. The existing `push(vec![..])` blocks are unchanged, so most of the churn from rewrapping is gone.
- Attack trees go back to the original loop. The attack steps loop skips a tree that is not in the scope (`sys.shows(|u| Some(u.tree(idx)))`).
- Problems keep the loop variable `p` and use `ws.project()` inside the closure, so the lines are no longer renamed.
- Behaviour is the same as the first round: same tests, same output.
- Counted lines from base 1bcd981 (`git diff --numstat`, not tests): xlsx.rs 184+5, mod.rs 4+1, session.rs 4+3, systems.rs 15+0, so 216 in total.

Done when (rerun):
1. `cargo test --locked -p aegis-client --test system_reports`: ok, 11 passed.
2. `cargo test --locked -p aegis-client --test report_builder --test reduced_by_report --test pages`: ok (22, 14, 14).
3. `cargo check --locked -p aegis-shell`: ok.
Before commit: `cargo fmt --all` ok; `scripts/ci.sh crates/aegis-client`: fmt, clippy and test all ok.

## Rework 2

Review: (1) `recount` matched Overview rows to sheets by name; (2) the filtered-workbook test covered three of fifteen filters.

Commit: 5b11bfb Count each Overview row from its own sheet and test every workbook filter (121b)
Files: crates/aegis-client/src/report/xlsx.rs, crates/aegis-client/tests/system_reports.rs

1. `recount` is deleted. `report_workbook_xlsx_in` keeps `counts: Vec<usize>` and pushes the data-row count of each element sheet just before it is pushed, in Overview order (components, interfaces, data flows, trust boundaries when `has_item`; then assets, damage scenarios, threats, attack trees, controls, goals, claims). When `sys.scope.is_on()`, Overview row `i + 1` takes `counts[i]`. New test `each_overview_count_is_its_own_sheet_when_two_kinds_share_a_name`: goals and claims both named "Requirements"; a Brake claim is added, so the Brake view has 0 goals and 1 claim; the two Overview "Requirements" rows read 0 and 1, the data rows of "Requirements" and "Requirements (2)".
2. `a_filtered_workbook_holds_only_the_filter` now, for every sheet except Overview, asserts that each ADAS-only name is in some cell of the whole workbook and in no cell of the Brake workbook. The one allowed cell is the "To component" column of the data flows sheet, since the Brake flow goes to the ADAS camera. The test adds two things the fixture lacked, inside the test only (so the 121a tests are unchanged): a step under the ADAS tree (a root-only tree has no Attack steps row) and an ADAS-only component and data flow (the fixture had no ADAS-only flow). It also asserts that the Attack steps sheet has a row for the ADAS tree in the whole workbook and none in the Brake one, and the same for Problems rows that name "ADAS camera". Security settings: the fixture's ADAS control applied to the ADAS camera puts an ADAS row there, so that filter is covered too.
   Checked by mutation: I turned off the filter of each of the 15 filtered sheets in turn (risk register, components, interfaces, data flows, trust boundaries, security settings, assets, damage, threats, attack trees, attack steps, controls, goals, claims, problems). Each change made `a_filtered_workbook_holds_only_the_filter` fail, and the goals and claims changes also made the new count test fail.

Lesson noted from review: count the rows where each sheet is built, not by looking the sheet up by name.

Done when (rerun):
1. `cargo test --locked -p aegis-client --test system_reports`: ok, 12 passed.
2. `cargo test --locked -p aegis-client --test report_builder --test reduced_by_report --test pages`: ok (22, 14, 14).
3. `cargo check --locked -p aegis-shell`: ok.
Before commit: `cargo fmt --all` ok; `scripts/ci.sh crates/aegis-client`: fmt, clippy and test all ok.
Non-test lines from base 1bcd981: xlsx.rs 187+5, mod.rs 4+1, session.rs 4+3, systems.rs 15+0.

NEXT: reviewer
