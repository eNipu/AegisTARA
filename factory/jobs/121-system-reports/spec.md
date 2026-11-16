# Spec: 121-system-reports

## Title

Reports and exports by system: a systems filter in the report profile and the export audit detail, the cover naming the systems, a Systems column in the XLSX sheets, and a System column in the Excel import.

## Design Reference

- Requirement: `docs/requirements.md` SYS-06 (line 172).
- Decision: `docs/decisions.md` D26 (line 203): a system is a named group of components; a view never changes a number.
- Design: `docs/brainstorm-systems-config.md` lines 208 to 217 (Reports; Excel export and import) and the test list for 121 (lines 497 to 501).
- Scope limit: `factory/jobs/119-systems/rework.md`, `## Answers`, answer 1 (no new Slint screen work; test the client).
- Code: `crates/aegis-client/src/report/profile.rs` (`ReportFilters`, `ReportProfile::audit_summary`), `report/doc.rs` (`build`, lines 441 to 1130), `report/xlsx.rs` (`report_workbook_xlsx_with_figures`, lines 339 to 757), `excel.rs` (`ExcelMapping`, `ExcelPreview`, `preview_import`), `systems.rs` (`SystemView`), `session.rs` (`export`, line 1356), `pages.rs` (`overview_in`, `comparison_in`); `crates/aegis-model/src/system.rs` (`SystemUse`, `SystemSet`); `crates/aegis-server/src/service.rs` lines 1069 to 1075 (the audit detail is printable ASCII, at most 256 bytes).

## Problem & Goal

Jobs 119 and 120 let one project hold several systems and show them one at a time or together on screen. The deliverables do not follow yet: a report always covers the whole project, the workbook does not say which system a row belongs to, and an imported spreadsheet cannot put components in systems. After this job the client can produce a report and a workbook for one system, a subset or the whole project, and an Excel import can build the systems.

Business value (P7):

| Part | Value |
|---|---|
| Systems filter saved in the report profile | The consultant delivers "the Brake TARA" and "the vehicle TARA" from one project, and repeats the same export next month without picking again. |
| Systems in the export audit detail | The evidence says what was handed over: the whole project or only Brake. |
| Cover names the systems | The reader of a PDF knows its scope from page one; a partial report cannot pass as the whole. |
| Systems column in XLSX | The customer filters and pivots by system in Excel; a shared damage scenario or control is one row with both names, so nothing is counted twice. |
| System column in the Excel import | A customer's existing spreadsheet with an "ECU / System" column becomes a project with systems in one step, not an hour of drag and drop. Same dry-run, same single undo step. |

## Decisions

- **The profile stores system names, not IDs.** `ReportFilters.systems: Vec<String>`; empty means all. Reason: the existing filters (`assets`, `component`) are names, and a company profile is used on several projects.
- **A name that is no system of the project is ignored.** If none is left, the report covers the whole project. The cover and the audit detail show what was actually covered, so a stale profile cannot produce a report that claims a scope it does not have.
- **Names match exactly** (as the asset filter does). Two systems with the same name are both included.
- **The report filter is the profile's, not the on-screen view.** Reason: the view is per user and never recorded (SYS-03); the profile is recorded in the audit event.
- **Audit detail form:** ` systems=Brake+ADAS` appended to the existing summary. Each name is reduced to ASCII letters, digits, `-` and `_` (other characters become `-`, outer `-` trimmed, an empty result becomes `x`), at most 8 names, then `+N` for the rest, and the whole summary is cut to 256 bytes. Reason: the server refuses a detail that is not printable ASCII or is longer than 256 bytes, and then the export would not be recorded at all.
- **The cover row appears only when the project has systems.** With all systems it says "All systems". A project with no systems gets the cover it has today (SYS-01).
- **The XLSX column is named "Systems", is the last column of each element sheet, and exists only when the project has systems.** Reason: column positions that existing users and the import's auto-detect rely on do not move, and a project with no systems exports exactly as today. Several names are joined with ", " in ID order.
- **The workbook follows the same filter as the report.** Reason: SYS-06 says "reports and exports".
- **Import: the System cell is one system name**, trimmed, matched to existing systems without regard to case. It is used only when the row names a component; otherwise it is ignored without making the row unmapped. Reason: the workbook this app exports has a Systems column on sheets with no component column (risk register, damage scenarios), where the cell may hold several names; it must import again without inventing a system called "Brake, ADAS".
- **Import: an existing component that is already in the named system gets no op; one in another system is moved.** A child component cannot be moved (the model refuses it); if its inherited system is the named one nothing happens, otherwise the row goes to the unmapped list with the model's reason, as every refused row does.
- **Counting:** `ExcelPreview.systems_to_add` counts new systems; each `SetComponentSystem` counts in `updates_and_links`. The summary sentence gains "N system(s)" after the components.

## Acceptance Criteria

1. A `ReportProfile` with `filters.systems` survives a JSON round trip unchanged, and a profile JSON written before this job (no `systems` key) loads with an empty list. Check: `cargo test --locked -p aegis-client --test system_reports`.
2. `audit_summary()` of a profile with systems contains `systems=` with the names in the form decided above, is printable ASCII, and is at most 256 bytes even with 64 long non-ASCII names; a profile with no systems gives exactly the summary it gave before. Before an export the session drops the names that are no system of the project, so the detail names only what was covered. Check: same command.
3. The report cover has a "Systems" row: the names of the systems covered, or "All systems"; a project with no systems has no such row. English and Japanese. Check: same command.
4. A report for one system contains the components, interfaces, data flows, trust boundaries, security settings, assets, damage scenarios, threats, attack trees, risks, controls, goals, claims and problems that show in that system (`SystemSet::shows_in`) and none that belong only to another; the executive summary counts cover the same part. A report for two systems lists a damage scenario and a control used by both once. On a project with no systems, a profile that names a system gives the same HTML as a profile that names none (a stale name is ignored). The expected rows are computed in the test from the fixture's own lists, not from the report. Check: same command.
5. In a project with systems, the workbook sheets for the risk register, components, interfaces, data flows, trust boundaries, assets, damage scenarios, threats, attack trees, controls, goals and claims end with a "Systems" column; a shared damage scenario shows "ADAS, Brake" style text with both names; a project with no systems has no "Systems" header on any sheet. Check: same command, plus `--test report_builder --test reduced_by_report --test pages` still pass.
6. The workbook built with a systems filter holds only the rows that show in the filter, and its Overview sheet counts match the rows of the sheets. The session passes the profile's filter to the workbook. Check: same command.
7. Excel import: the header "System", "Systems" or "システム" is auto-detected. A sheet with Component and System columns gives a preview with the right `systems_to_add`, one `AddElement` of kind System per new name (a repeated name, or one that differs only in case, creates one system), and a `SetComponentSystem` per component; the project is unchanged by the preview; applying `preview.ops` as one batch puts every component in its system and one undo removes all of it. An existing system is reused; a component already in the named system gets no op. Check: same command.
8. Excel import: a System cell on a row with no component creates nothing and does not make the row unmapped; a workbook exported by this app from a project with systems imports into an empty project with the same components in the same systems. Check: same command.
9. The whole workspace passes the gate, including `aegis-shell`, which this job does not change. Check: `scripts/ci.sh`.

## Out of scope

- **The screen, left for the GPUI shell** (owner rule): the systems picker in the report builder and its keyboard reachability, the System entry in the import mapping dialog, and their strings. No `.slint` file and no Slint glue in `aegis-shell` changes. The client data is ready: `ReportFilters.systems`, `systems::system_choices`, `ExcelMapping.system_col`.
- A "System" column in the report's own tables (PDF and HTML); the cover states the scope.
- The CSV risk register and the tree SVG export.
- Importing parent and child components, or several systems per cell.
- Per-system sub-totals inside the report (the overview page of job 120 has them).
- Any server change; the audit detail limit stays 256 bytes of printable ASCII.
