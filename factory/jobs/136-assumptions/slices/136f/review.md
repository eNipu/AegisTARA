VERDICT: PASS
## For the owner
What changed: Added cybersecurity assumptions to Word/HTML/PDF reports (cover holding count, Assumptions table with linked claims and exclusions, and assumption descriptions) while preserving existing block offsets when a project has no assumptions. Updated XLSX workbooks and risk-register exports so `"Excluded"` columns and `"not applicable"` risk cells appear for holding assumptions even without conditions, and added an `"Assumptions"` sheet routed through `cell_text` formula-injection escaping. Added trilingual Fluent keys (`report-assumptions`, `report-assumptions-value`) and integration/unit tests across `en`/`ja`/`pseudo`, mixed condition+assumption projects, and formula-injection payloads.
Look at:
- `crates/aegis-client/src/report/doc.rs:1325` — widened `"Configuration and assumptions"` gate (`has_conditions || has_assumptions`), guarded `"Conditions"` sub-table, and appended `"Assumptions"` table + `Described::Assumptions` after `"Excluded elements and steps"`.
- `crates/aegis-client/src/report/xlsx.rs:328` — `has_applicability = has_conditions || has_assumptions` controls `"Excluded"` columns and `"not applicable"` cells in both `risk_register_sheet` and `report_workbook_xlsx_in`, plus the new `"Assumptions"` sheet at line 1002.
- `crates/aegis-client/tests/assumptions.rs:1164` — tests assumption-only/mixed Word+XLSX reports, `"Excluded"` columns from assumptions alone, and `=`, `+`, `-`, `@` formula-injection escaping.
Risk: none — all string cells go through `worksheet_xml`/`cell_text` (`guard` + `xml_escape`), link lists iterate in deterministic project order, and `report_configuration` plus `assumptions` and `scripts/ci.sh` passed in the gate.
View: git -C factory/worktrees/136-assumptions diff f6f5f68cbfd031942cdcd514f181ad00f5257edc..HEAD
## Findings
- none
## Lesson
none
NEXT: human
