VERDICT: PASS
## For the owner
What changed: The exported workbook now ends every element sheet with a "Systems" column when the project has systems; a row shared by two systems stays one row with both names.
The workbook export follows the report's systems filter: rows of other systems are left out of all fifteen filtered sheets, and each Overview count is the number of rows left in its own sheet.
A project with no systems gets no new column and no filtering.
Look at:
- crates/aegis-client/src/report/xlsx.rs:925-935: the Overview counts are written by position from `counts`, which is filled in the same order as the Overview rows are built (lines 444-483). A new Overview row or a new element sheet added later must be added in both places, in the same order, or the counts shift by one with no error.
- crates/aegis-client/src/report/xlsx.rs:881: a problem with no target, or whose target is gone, is always kept in a filtered workbook. That is what the prompt asks; decide whether a customer who exports one system should see project-wide problems.
- crates/aegis-client/src/session.rs:1405: a filter name that matches no system exports all systems, with no message (the rule from 121a).
Risk: Low. Three things are not covered and were not asked for: (1) the Figures sheet is not filtered, so a figure attached to an element of another system still lists its ID, caption and target in a one-system workbook; (2) a system name that contains ", " makes the Systems cell ambiguous, so the import slice must not split that cell on ", "; (3) the byte-for-byte check in `a_project_with_no_systems_exports_as_before` compares the new function with a wrapper around itself and cannot fail, so "same bytes as before the change" rests on the "no Systems header" assertion in the same test and on the existing report_builder workbook tests, not on a stored old file. Checked by reading every changed line: "Systems" is pushed after every other header on all twelve sheets, names are in ID order, the filter test asserts each ADAS-only name is present in the whole workbook and absent from the Brake one (so it fails when any one sheet's filter is removed; the builder reports doing that for all fifteen), and the repeated-name test fails if counts are matched by sheet name. No new input boundary, dependency, path or map keyed by input; system names go through the same cell writer as every other name.
View: git -C factory/worktrees/121-system-reports diff 1bcd981c6a4e93e477efc0df2755331663250b9c..HEAD
## Findings
- none. Both findings of the earlier review are fixed: `recount` is gone and counts are taken where each sheet is built (xlsx.rs:513, 540, 561, 581, 634, 672, 738, 766, 827, 853, 872), and the filter test now covers every filtered sheet.
- Note, not a defect: `Rows::new` and `Scope::new` each call `project.system_use()` when a filter is on, so a filtered export computes it twice. It is O(project) once per export; leave it.
## Lesson
[planner] "Equals the old function byte for byte" proves nothing when the old function now calls the new one; ask for a check that does not go through the changed code (no new header, or a stored expected value).
NEXT: human
