VERDICT: PASS
## For the owner
What changed: ExcelMapping gained a responsible column with en/ja aliases, and the import now turns each comma/`、`-separated name into a new-or-reused party linked to the row's goal/claim inside the same per-row batch. The XLSX workbook gained a Parties sheet and a Responsible column on Goals/Claims, and the DOCX report gained a Responsible header/cell in both tables. Tests cover auto-detect, import ops (dedup + cap), the Parties sheet and the formula guard, and the report header.
Look at:
- crates/aegis-client/src/excel.rs:1389-1433 — new Responsible op construction: dedup by lowercase, cap at MAX_RESPONSIBLE, AddParty(kind Other) then LinkGoal/LinkClaimParty; confirm all ops land in the same row_ops batch.
- crates/aegis-client/src/excel.rs:1511-1540 — the shared dry-run + rollback: a row whose new party/link op fails (UnknownParty, cap, duplicate) is rolled back and reported as unmapped, i.e. refused atomically like every other column.
- crates/aegis-client/src/report/xlsx.rs:105-126 — worksheet_xml routes every cell through guard()+xml_escape(), so the new Parties/Responsible cells inherit the formula-injection guard without a new code path.
Risk: Low. The new ops join the existing per-row simulation (excel.rs:1511-1540), so a row naming a party that cannot be created/linked is refused as one batch and the model guards (UnknownParty, TooManyGoal/ClaimResponsible, DuplicateGoal/ClaimParty) hold; the cap/dedup and the leading-'=' escape are asserted in the new unit tests. en/ja `responsible` keys and the `責任者`/`担当` header aliases both exist. Two non-blocking notes: a row with a Responsible cell but no goal/claim still creates an unlinked party; and the 16-name cap is not pre-counted against links already on a goal/claim, so importing into a pre-linked project can trip the model cap and safely refuse that row via the batch. Neither is exercised by the fresh-project tests but both fail safe.
View: git -C factory/worktrees/145-parties diff 43caab9d6cec3ce91ca56e83fd00ee97ca87bc4b..HEAD
## Findings
none
## Lesson
[planner] Prompt 145e called the `updates_and_links` display counter an "op-validation list" (item 3); real validation is the per-row simulation at 1511-1540 — name the exact mechanism so a builder does not wire the counter as a guard.
NEXT: human
