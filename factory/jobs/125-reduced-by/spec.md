# Spec: 125-reduced-by

## Title

"Reduced by" in tables, reports and XLSX (CFG-07, D26)

## Design Reference

- `docs/requirements.md`, CFG-07 (line 188).
- `docs/decisions.md`, D26 (line 203): the engine derives the ceilings and the explain-why names them.
- `docs/design/systems-config.md`, section 4 (line 125, "caps 3 steps · lowers 1 risk"), section 7 (line 241, deciding ceiling first, others under "also"), section 9 (line 312, the risk table column), section 13 (line 395, English and Japanese text).
- `factory/jobs/125-reduced-by/rework.md`, `## Answers`: no Slint work in this job.
- Code:
  - `crates/aegis-model/src/control.rs` (`CeilingSource` line 157, `StepCeiling` line 233), `crates/aegis-model/src/project.rs` (`step_ceilings` line 3264, `residual_step_feasibility` line 3182, `evaluate_residual_tree` line 3656), `crates/aegis-model/src/eval.rs` (`why_path` line 521), `crates/aegis-model/src/lib.rs` (line 263).
  - `crates/aegis-client/src/pages.rs` (`ceiling_lines` line 1263, `RiskRow` line 1445, `risk_rows` line 1473, `ControlRow` line 1609, `control_rows` line 1678, `SettingRow` line 398, `format_rules` line 617).
  - `crates/aegis-client/src/report/doc.rs` (`risk_rows_block` line 182, item definition line 630, controls line 979), `crates/aegis-client/src/report/xlsx.rs` (`guard` line 28, `risk_register_sheet` line 275, threats sheet line 510, controls sheet line 618), `crates/aegis-client/locales/{en,ja,pseudo}/main.ftl`.

## Problem & Goal

After jobs 123 and 124 a security setting caps attack steps and the residual risk falls. But the only place that says which setting did it is the detail of one threat. The risk table, the control table, the report and the Excel export show the lower number with no reason. A consultant cannot answer the client's first question, "which measure brought this risk down, and which measures do nothing?", without opening every threat.

Goal: every place that lists risks names the settings on the deciding path, every place that lists controls says how many risks each one lowers, and the report and the workbook list the settings of each component and interface.

Business value of each part (P7):

| Part | Value |
|---|---|
| `Project::deciding_step` in the model | One definition of "the deciding step and what caps it", used by the client, the report and later the server API. No second copy that can drift from the engine. |
| Reduced by on a risk row | The reviewer sees the reason for each residual number in the table. When nothing caps the deciding step, the row names that step, which is where to act next. |
| Lowers N risks on a control row | Shows which measures earn their cost and which change nothing. This is the "what does this measure buy us" answer a consultant gives a client. |
| "Security settings" in the report | The work product states the configuration the numbers depend on, per component and interface. An auditor can check it against the real ECU. |
| XLSX columns and sheet | Clients keep risk registers in Excel. The reason travels with the number. |
| Formula guard test on the new cells | A setting or step name that starts with `=` must not become a live formula in the client's spreadsheet. |

## Decisions made here

1. **What "on the deciding path" means.** The why path of the residual evaluation (`Evaluation::why_path`) is one chain from the root to one step. Only steps carry ceilings. So the settings on the deciding path are the ceilings on that last step that are below the step's own rating. Their order is the order of `Project::step_ceilings`: the deciding (lowest) ceiling first, ties by control id. This is "why-path order".
2. **A ceiling at or above the step's rating is not listed.** It caps nothing. This is the rule `ceiling_lines` in `pages.rs` uses today.
3. **A control is listed once per risk.** A setting applied to an interface and to its component reaches one step twice; the row keeps the first (lowest) entry.
4. **Nothing caps the deciding step.** The row has no settings and names the deciding step. A threat with no attack tree has neither.
5. **"Lowers N risks".** A control lowers a risk when the risk after controls is lower than the risk before controls and the control is in that row's Reduced by list. So the count is exactly the number of risk table rows that name the control and whose risk fell. Known limits, accepted (P3): a control that caps only a path that does not decide is not counted (that row names the deciding step instead), and a second, weaker setting on the same step is counted. A count that asks "would the risk rise without this control" needs one evaluation per control and threat; it can replace this rule later without changing the view data.
6. **The text of one entry is language-neutral:** `Secure boot (ADAS ECU)` for a setting, `Secure boot` for a control attached to the step by hand. No "on" or "from" word, so the same text is right in an English and a Japanese report.
7. **The report section is part of the item definition.** A "Security settings" table follows the trust boundaries when the item definition section is on and at least one setting is applied. No new switch in `ReportSections`: the report profile format does not change.
8. **The table lists direct applications only.** One row per component or interface and setting applied to it. One sentence under the heading says that a component's settings also apply to its interfaces. Repeating each component setting on each of its interfaces would multiply the rows without adding a fact.
9. **XLSX:** a "Reduced by" column in the "Risk register" sheet and in the threats sheet, a "Risks lowered" column in the controls sheet, and a "Security settings" sheet after the trust boundaries. All cells go through the existing `guard` in `xlsx.rs`; no new guard code.
10. **The CSV risk register does not change.** It is the old single-file export; adding a column would break its users for no request.

## Acceptance Criteria

1. `Project::deciding_step(tree, residual_evaluation)` returns the step that decides the tree, its rating, its residual value and the ceilings below the rating, deciding one first. It returns `None` for an unknown tree, an evaluation of another tree, or a path that does not end at a rated step. Check: `cargo test --locked -p aegis-model --test reduced_by`.
2. `pages::risk_rows` gives, per row, `reduced_by` (control, visual id, name, where it is applied) in why-path order, `deciding_step` (tree, node, label) and `lowered`. Check: `cargo test --locked -p aegis-client --test reduced_by`.
3. `pages::reduced_by_cell` gives `Secure boot`, `Secure boot +1`, `— (Inject on CAN)` or `—`; `pages::reduced_by_text` gives `Secure boot (ADAS ECU); Signed updates` or `None`. Check: same test.
4. `pages::control_rows` gives `lowers` per control, and `pages::lowers_label` gives `Lowers no risk`, `Lowers 1 risk`, `Lowers 3 risks`. Check: same test.
5. The lines "Step ... is capped at ... by ..." in the threat detail are unchanged. Check: `cargo test --locked -p aegis-client --test pages`.
6. The report's risk table has a "Reduced by" column, the controls table a "Risks lowered" column, and the item definition a "Security settings" table with one row per component or interface and setting; the Japanese report uses 低減策 and セキュリティ設定 and the pack's Japanese rule labels; a project with no applied setting has no "Security settings" heading. Check: `cargo test --locked -p aegis-client --test reduced_by_report --test report_builder`.
7. The workbook has the "Reduced by" columns, the "Risks lowered" column and the "Security settings" sheet; a name that starts with `=`, `+`, `-` or `@` is written with a leading `'` in every new cell; the workbook still imports. Check: `cargo test --locked -p aegis-client --test reduced_by_report --test report_builder`.
8. `aegis-shell` still builds with no change to its files. Check: `cargo check --locked -p aegis-shell`.
9. The whole workspace passes. Check: `scripts/ci.sh`.

## Out of scope

- **The screen.** The "Reduced by" column in the risk table, the "Lowers N risks" column in the control table, the "caps N steps · lowers M risks" line under a setting in the component and interface inspectors, and the "Reduced by Secure boot on ADAS ECU" filter chip are drawn by the GPUI shell in a later job. This job gives that shell the view data: `RiskRow::reduced_by` (with the control id to filter on), `RiskRow::deciding_step`, `ControlRow::lowers`, `pages::reduced_by_cell` and `pages::lowers_label`. No `.slint` file and no Slint glue in `aegis-shell` changes.
- A headless screen test of the column. The client test of the column data replaces it (rework answer).
- "caps N steps · lowers M risks" per setting and target (design section 4). It needs a count per application, not per control; it ships with the inspector in the GPUI shell.
- A counterfactual "lowers" count (decision 5).
- The CSV risk register (decision 10).
- A separate report switch for the settings table (decision 7).
- Control status Implemented, Planned, Off (CFG-10, job 140).
