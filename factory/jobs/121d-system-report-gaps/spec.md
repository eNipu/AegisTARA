# Spec: 121d-system-report-gaps

## Title

System report gaps (follow-up to 121, SYS-06; what the 121 reviewers were unsure about): (1) figures follow the systems filter: the Figures sheet of the workbook (`report/xlsx.rs`) and the figures list of the report (`report/doc.rs`) leave out a figure whose target element does not show in the chosen systems; figures on the project, the item description or the item scope, and figures with an unreadable target, stay; (2) a stale system name stops the export: when the profile names a system that is not a live system, the export in `session.rs` writes nothing and shows a notice that names the missing systems, instead of silently exporting the whole project; (3) a warning in the Problems list when two systems share a name, never a block; (4) tests that can fail for what 121 only read: the report's problems-table filter, the comparison called with the view, the stale-name stop, an import row refused after it would have created a system (a later row with the same name creates it exactly once), and the import refusing a 65th system with the model's reason; (5) remove the self-comparison line from `a_project_with_no_systems_exports_as_before`. No screen work; names containing ", " stay as they are; tests: a figure on an ADAS-only component is absent from the Brake report and workbook and present in the full ones (121d-system-report-gaps)

## Design Reference

- Requirement: `docs/requirements.md` SYS-06 (line 172); SYS-01 (line 167: with no systems the app behaves as today); SYS-02 (line 168: the linter flags system problems).
- Decision: `docs/decisions.md` D26 (line 203): a system is a named group of components; a view never changes a number.
- Previous job: `factory/jobs/121-system-reports/spec.md` (decisions on names, the stale-name rule, the workbook column) and the three reviews in `factory/jobs/121-system-reports/slices/121{a,b,c}/review.md` (the open points this job closes).
- Code: `crates/aegis-client/src/systems.rs` (`view_of_names` 195-208, `Scope` 220-272, `selection_systems` 415-440), `crates/aegis-client/src/view.rs` (`target_to_element_ref` 523-546), `crates/aegis-client/src/report/doc.rs` (figures list 543-574, problems 1143-1167, comparison 1169-1189), `crates/aegis-client/src/report/xlsx.rs` (`Rows` 288-345, Figures sheet 894-923), `crates/aegis-client/src/session.rs` (`export` 1356-1433), `crates/aegis-client/src/pages.rs` (`problem` 2396, `problems` 2774, `comparison_in` 3240), `crates/aegis-client/src/excel.rs` (system column 1146-1176), `crates/aegis-model/src/lint.rs` (`LintCode`), `crates/aegis-model/src/project.rs` (`lint` 5431, `lint_components` 5702), `crates/aegis-model/src/element.rs` (`MAX_SYSTEMS` = 64, line 23; capacity error 657-661).

## Problem & Goal

Job 121 made reports and workbooks follow a systems filter, but its reviewers left three gaps and several lines that no test could catch. A Brake report still lists the captions and targets of figures attached to ADAS elements. A saved "Brake TARA" profile whose system was renamed silently exports every system. Two systems with one name are both picked by that name with no hint. And the problems filter, the comparison view, the stale-name handling and two import refusals were checked by reading only.

After this job a filtered report and workbook leave out other systems' figures, an export never claims a scope the profile did not ask for, the Problems list warns about a shared system name, and each of those lines has a test that fails when the line is removed.

Business value (P7):

| Part | Value |
|---|---|
| Figures follow the filter | A Brake report handed to the Brake supplier does not leak the names and captions of ADAS figures. |
| Stale name stops the export | A consultant who renamed a system gets told before a "Brake TARA" goes out holding the whole vehicle; nothing wrong is written or recorded. |
| Shared-name warning | The user learns why a profile naming "Brake" covers two systems, and can rename one; nothing is blocked. |
| Tests for read-only lines | The next change to the report, the comparison or the import cannot break these rules unnoticed. |
| Remove the self-comparison | A test line that cannot fail no longer suggests a guarantee that does not exist. |

## Decisions

- **A figure shows when its target shows.** Targets `""`, `project`, `item_description`, `item_scope`, a target `view::target_to_element_ref` cannot read, and a target whose element is gone always show. Otherwise the figure shows when its element shows in the scope (`systems::selection_systems` + `Scope::shows`). Reason: the same rule every row of the report uses; the unknown cases stay because hiding them could hide a project-wide figure.
- **The figures section and the Figures sheet appear only when at least one figure is kept**, as today when there are none. Reason: an empty "Figures" heading tells the reader nothing.
- **The stale-name stop applies to the HTML report, the PDF report and the workbook only.** The CSV risk register and the tree SVG never use the systems filter and export as before. Reason: only those three outputs claim a system scope.
- **The stop applies only when the project has systems.** A project with no systems exports as today even if the profile names one. Reason: SYS-01 (with no systems the app behaves as today) and a company profile is shared by several projects (121 decision); with no systems the cover has no Systems row and the audit detail no `systems=`, so the output does not claim a scope.
- **A missing name is one that equals no live system name exactly**, the same match the report filter uses. The notice lists each missing name once, in profile order, quoted: `Nothing was exported: no system of this project is named "Gone", "Old". Change the systems in the report profile.` Severity Warning. English only, like every other session notice.
- **The report builders still ignore a stale name** (`report_html_with_options` and friends); only the session's export stops. Reason: the builders are also used by tests and previews that have no notice to show; 121's test `a_stale_system_name_is_ignored` stays.
- **Shared-name lint: a new model lint code `LintCode::SystemNameShared { system, first }`, severity Warning.** Names match exactly (case matters), the same rule as the report filter; for each group of systems with one name, every system but the one with the lowest ID gets one finding pointing at that lowest ID. Reason: the linter is where SYS-02 puts system problems; the import already merges names that differ only in case, so case-only duplicates come only from typing and do not confuse the filter.
- **The Problems row of a shared name has no target** (`ProblemRow.target: None`), visual ID of the later system, text `System SYS01 "Brake" has the same name as SYS00; a report profile that names "Brake" covers both.` Reason: `Selection` has no system variant, and a row with no target already shows in every system's report.
- **Names containing ", " are not checked or changed** (backlog).
- **Two slices**: client report and export work first, then the model lint plus the remaining tests. Reason: independent code, each about 250 to 300 changed lines with tests.

## Acceptance Criteria

1. A figure on an ADAS-only component is absent from the figures table of the Brake report and from the Figures sheet of the Brake workbook, and present in both full outputs; figures on the project, the item description, and with an unreadable target are present in both. Check: `cargo test --locked -p aegis-client --test system_reports`.
2. `systems::missing_system_names` returns the names that equal no live system, each once, in input order, and nothing when the project has no systems. Check: same command.
3. Through a running session: exporting an HTML report with a profile naming `Brake` and `Gone` in a project with a `Brake` system writes no file and shows a Warning notice containing `"Gone"` and not `"Brake"`; the same export naming only `Brake` writes the file; on a project with no systems, a profile naming `Brake` writes the file. Check: `cargo test --locked -p aegis-client --test session a_stale_system_name`.
4. `a_project_with_no_systems_exports_as_before` no longer compares `report_workbook_xlsx_in` with `report_workbook_xlsx`. Check: `grep -c "report_workbook_xlsx(&ws)" crates/aegis-client/tests/system_reports.rs` counts only the use in `an_exported_workbook_imports_with_its_systems` (1).
5. Two systems with the same name give one `SystemNameShared` warning (never an error) naming the later and the first; three give two; names that differ only in case, or differ, give none. Check: `cargo test --locked -p aegis-model --test systems`.
6. The Problems list has a warning row with no target for a shared name, and the Brake report's problems table holds that row, holds no problem about the ADAS camera, while the full report does. Check: `cargo test --locked -p aegis-client --test system_reports`.
7. With a baseline, the Brake report's comparison names an added Brake component, not an added ADAS component, and its summary says "outside this view"; the full report names both and does not. Check: same command.
8. Excel import: a row that would create system `Spare` but is refused (a child component) adds nothing; a later row naming `Spare` creates it exactly once and places its component in it. A project with 64 systems refuses a row naming a 65th with `ModelError::ProjectCapacityExceeded { kind: EntityKind::Element }` as the reason. Check: same command.
9. The whole workspace passes the gate. Check: `scripts/ci.sh`.

## Out of scope

- Any screen change (`.slint`, Slint glue in `aegis-shell`): no "which systems are missing" picker, no rename prompt.
- Changing the report builders' rule for stale names, the audit detail, the cover, the CSV or the SVG export.
- Splitting or validating system names that contain ", ".
- Case-insensitive duplicate detection, and blocking a duplicate name in the model.
- Any server or snapshot format change.
