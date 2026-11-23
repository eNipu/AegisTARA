VERDICT: PASS
## For the owner
What changed: Added configuration summary rows on the report cover and an unnumbered Configuration and assumptions section (conditions table plus excluded elements and leaf steps) shared by HTML and PDF reports whenever a project defines conditions. Added a Conditions sheet, an Excluded column across the risk register, Components, Interfaces, Assets, and Threat scenarios sheets, and "not applicable" risk cells for excluded threats in XLSX exports, keeping projects without conditions unchanged. Synced `aegis-shell` page indices and `ja`/`pseudo` PO translations for `Page::Configuration` so workspace CI passes.
Look at:
- `crates/aegis-client/src/report/xlsx.rs:226-247` — `excluded_cell` and `risk_cell` drive `"not applicable"` risk values and `"Excluded"` columns before `sys.head`/`end_row` so `"Systems"` remains last.
- `crates/aegis-client/src/report/xlsx.rs:975-995` — `Conditions` sheet is emitted after `Problems` without pushing to `counts`, preserving `overview.rows.skip(1).zip(&counts)` positional alignment.
- `crates/aegis-shell/ui/app.slint:22-37` — `Pages` indices shifted after inserting `configuration: 3`, matching `Page::ALL` in `crates/aegis-client/src/view.rs:521-534`.
Risk: A report profile or workbook consumer expecting fixed column counts on element sheets when conditions are present will see the new `Excluded` column before `Systems`; checked against projects with no conditions (unchanged output), projects with conditions (all 5 sheets + standalone risk register + HTML/PDF), and formula-injection prefixes (`=HYPERLINK("x")`) in `tests/report_configuration.rs`, with `scripts/ci.sh` passing after the final commit.
View: git -C factory/worktrees/135-applicability-ui diff 0a0a674f3d19443b30760517da387022947a7348..HEAD
## Findings
none
## Lesson
none
NEXT: human
