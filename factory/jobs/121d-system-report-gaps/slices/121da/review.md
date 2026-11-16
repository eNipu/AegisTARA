VERDICT: PASS
## For the owner
What changed: A report or workbook filtered to some systems now lists only the figures whose element shows in that filter. Figures on the project, the item description, the item scope, an unreadable target or a deleted element always stay.
An HTML, PDF or workbook export whose profile names a system the project no longer has now writes nothing and shows a warning that names the missing systems. CSV and SVG exports, and projects with no systems, work as before.
A test line that compared the workbook builder with itself was removed.
Look at: crates/aegis-client/src/session.rs:1379 (the new stop, which runs before the copy and the blocking build; the warning quotes the profile's names); crates/aegis-client/src/systems.rs:466 (`figure_shows`: `None` from parsing or from an element with no systems means "show", which is the rule the prompt asked for); crates/aegis-client/src/report/xlsx.rs:894 (the filter uses the outer `p`, the project; the Problems loop's `p` is out of scope here, so this is the right one).
Risk: Low. The new stop could block a valid export if `missing_system_names` and `view_of_names` matched names in different ways. Both use exact equality (systems.rs:205 and the new function), and the test checks that "brake" (lowercase) counts as stale. The warning quotes names from the user's own profile. Their number is bounded by the systems the profile was built from, and they only appear in a local notice, never in logs or the audit. The builder tried removing each filter line and the stop by hand, and the tests failed each time.
View: git -C factory/worktrees/121d-system-report-gaps diff e7b38ffec55e50e28169488b3abdccf6212f56d9..HEAD
## Findings
none
## Lesson
none
NEXT: human
