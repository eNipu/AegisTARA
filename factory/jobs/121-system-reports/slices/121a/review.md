VERDICT: PASS
## For the owner
What changed: A report profile can name systems; the report then keeps only rows that show in those systems.
The cover gets a "Systems" row (the names, or "All systems"), and the export audit detail gets ` systems=A+B`, capped at 256 bytes.
Export drops names that are no live system before it writes the audit detail and the report.
Look at:
- `crates/aegis-client/src/systems.rs:180-192` (`view_of_names`): a profile whose names match no system gives the WHOLE project, not an empty report. If "Brake" is renamed, a saved "Brake TARA" profile exports every system; the cover then reads "All systems" and the audit detail has no `systems=`, so the evidence is honest, but nothing warns the consultant. This is what the prompt asked for.
- `crates/aegis-client/src/report/profile.rs:310-326`: the audit detail is now cut to 256 bytes. With long ASCII system names the cut can fall inside a name or drop the `+N` count, so the detail then names fewer systems than the report covers. The cover of the report itself is always complete.
- `crates/aegis-client/src/report/doc.rs:543-573`: the figures list still lists every figure of the profile, including figures attached to elements of systems left out (caption and target text only). Not asked for in this slice.
Risk: Read every changed source line. Every table filters with the index function of its own kind; the scope is off (no row dropped, `system_use()` not computed) when no live system is named; the audit detail is ASCII before the byte cut, so the cut cannot panic and the detail is always printable. After rework 1 the tests cover every filter but three: the builder replaced each filter by `true` one at a time and the tests failed for components, interfaces, data flows, trust boundaries, security settings, assets, damage scenarios, attack trees, threats, controls, goals, claims and both executive-summary numbers. Checked by reading only, with no test that can fail: the problems table filter (`doc.rs:1150-1154`), the comparison call with the view (`doc.rs:1174-1179`), and the five lines in `session.rs:1384-1388` that drop stale names before export. Cells inside a kept row can still name an element of another system (a data flow's far end, a shared control's targets); that is expected. Two systems with the same name are both selected by that name and the cover lists the name twice.
View: git -C factory/worktrees/121-system-reports diff 37f8f710d9d87f697cbc186216a21587acb2898f..HEAD
## Findings
- none. Rework 1 fixed the three findings of the first review: the fixture now holds an ADAS-only trust boundary, attack tree, goal, claim and applied setting, each checked absent in the Brake report and present in the full report; the summary counts are asserted as (2, 2) for Brake and (3, 3) for the full report; a stale name is tested on a project that has systems, with an `assert_ne` so the equalities cannot pass trivially.
## Lesson
[planner] When a prompt says "filter every table", the test fixture it specifies must hold one element of every kind filtered, and name a check for every computed number; otherwise half the filter lines ship with no test that can fail.
NEXT: human
