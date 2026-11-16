VERDICT: PASS
## For the owner
What changed: the model linter now warns (never errors) once for each system whose exact name matches a system with a lower ID, and points at that first system.
The Problems list shows this warning as a row with no target, so it appears in every system's report.
Four new client tests check the report's problems filter, the comparison view, an import row refused after it planned a new system, and the refused 65th system.
Look at: crates/aegis-model/src/project.rs:5716 (`lint_systems`: names must match exactly, including case. The report view at systems.rs:205 matches names the same way, but the Excel import matches system names case-insensitively, so "brake" in a workbook goes to "Brake" and gives no warning. The prompt chose this on purpose.)
  crates/aegis-client/src/pages.rs:2775 (`shared_system_name`: the row's `target` is `None`, which the report's problems filter always keeps.)
  crates/aegis-client/tests/system_reports.rs:1165 (the refused-row test: excel.rs:1151 pushes `AddElement` before the row is simulated and refused, so this test really checks that the name map grows only after a row succeeds.)
Risk: low. The change only adds a lint variant and its Problems row, and the model refuses nothing new. The compiler checks that every match over `LintCode` covers the new variant, and the unit test covering every variant passes. The builder broke both guarded lines in report/doc.rs by hand: the problems test and the comparison test each failed, then passed again once the lines were put back.
View: git -C factory/worktrees/121d-system-report-gaps diff 106a65fe8ed37dc1443d4a1f3fc09bc5390ccf60..HEAD
## Findings
- none. All seven "Done when" items are met. Item 1's check filter `shared_name` does not match the test `a_shared_system_name_warns_once_each_after_the_first` (the name contains `shared_system_name`), so that check command runs only one of the two tests. Both tests run in the gate's full `--test systems` run and pass. The builder noted this in build.md.
## Lesson
[planner] A "Done when" check that runs `cargo test <filter>` must use a filter that is a substring of every test name it is meant to run; check each test name against the filter before writing the prompt.
NEXT: human
