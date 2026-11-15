VERDICT: PASS
## For the owner
What changed: the report's risk tables (executive summary and risk section) gain a "Reduced by" column naming the settings that lower each risk, or the deciding step when nothing does.
The controls table gains a "Risks lowered" count. The item definition gains a "Security settings" table listing each component, then its interfaces, with the settings applied directly to them and their rules.
Eight new text keys in English, Japanese and pseudo; nine new tests.
Look at:
- crates/aegis-client/src/pages.rs:438 `applied_settings`: ordering (components, then each one's interfaces) and the rule text in the report language, falling back to the default label.
- crates/aegis-client/src/report/doc.rs:206: the Reduced-by cell. With no reducer and no deciding step (a threat with no tree), the cell is empty. That is what the prompt asked for, but check whether an auditor would expect a word there.
- crates/aegis-client/src/report/doc.rs:732: the settings table only appears when the item definition section is on and at least one setting is applied.
Risk: low. The report gains columns, so any saved report or snapshot that compares tables by column count or position will change. The existing report_builder and pages tests still pass. The data comes from the project, not from outside input, and no dependency was added.
View: git -C factory/worktrees/125-reduced-by diff f859af580c97c0a8f4b1c4947316088c9da88b18..HEAD
## Findings
- None blocking. All nine "Do" items are met. Each test fails if its change is removed: it looks up the new column or heading by name, or the exact cell text. The "every key in every catalog" test checks against `[key]`, which is what `tr` returns for a missing key (i18n.rs:164). Nothing outside the allowed files was touched. The 108 counted lines are under the 250 cap.
- Note only: the keys were added at the end of each catalog, not next to `through`/`kind`. build.md explains why, and it has no effect on behaviour.
## Lesson
none
NEXT: human
