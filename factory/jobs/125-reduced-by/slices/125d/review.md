VERDICT: PASS
## For the owner
What changed: The Excel workbook now has a "Reduced by" column in the risk register and the threats sheet (the settings that lower the risk, or "Not reduced; deciding step: ..." when nothing does).
The controls sheet has a last column "Risks lowered", and a new sheet "Security settings" lists each component's and interface's settings with their rules.
Names typed by a user that look like formulas (=, +, -, @) are still written as text; a test checks the raw file for this.
Look at:
- crates/aegis-client/src/report/xlsx.rs:276 — `reduced_by_cell`, the wording a client reads in their risk register ("Not reduced; deciding step: <step>"; empty when the threat has no rated step).
- crates/aegis-client/src/report/xlsx.rs:471 — the "Security settings" sheet. Its name and headers are fixed English text, while the neighbouring item sheets take their names from the methodology; this is what the prompt asked for.
- crates/aegis-client/tests/reduced_by_report.rs:486 — the formula test. It checks the raw workbook bytes: every cell that starts with =, +, - or @ carries a leading apostrophe, anywhere in the file.
Risk: Low. Every new cell is a plain string in a `Sheet` row, so it goes through the existing `cell_text` (formula guard, then XML escaping); `guard`, `xml_escape`, `worksheet_xml` and the zip writer are unchanged in the diff. The builder reports that bypassing `guard` makes the formula test fail; I did not rerun that, but the raw-bytes assertion (`<t>=` must not appear) can only hold with the guard in place, because the fixture contains a control named `=HYPERLINK(...)`. Columns were only inserted at the named places; none renamed or reordered. The import of an exported workbook is covered only by "preview succeeds and adds at least one threat", the same strength as the existing test in report_builder.rs.
One thing that is not new and not fixed here: a guarded name reads back with its apostrophe (the test itself reads `'@SUM(A1)`), so exporting and re-importing a name that starts with =, +, - or @ adds an apostrophe to it. Nothing to decide for this slice.
View: git -C factory/worktrees/125-reduced-by diff 425a9180639a0f0874b1deb5c10cfd8117d177d9..HEAD
## Findings
- none. Notes, not defects:
- crates/aegis-client/tests/reduced_by_report.rs:453 — the "every data row has as many cells as the header row" check goes through `read_workbook`; if the reader pads short rows it proves less than it says. The column lookups by header position in the same test already catch a misplaced cell, so this is enough.
- crates/aegis-client/tests/reduced_by_report.rs:538 — the three "must contain" needles (`'=HYPERLINK(`, `'+ECU`, `'@SUM(A1)`) are also satisfied by sheets that existed before this slice (components, controls). The proof for the new cells is the file-wide "must not contain" check plus the read-back of the Reduced by cell, which are both sound.
## Lesson
none
NEXT: human
