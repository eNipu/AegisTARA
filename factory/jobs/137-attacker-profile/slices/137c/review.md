VERDICT: PASS
## For the owner
What changed: Exposed `Page::AttackerProfile` at `Page::ALL[9]` in group 4 (Attack feasibility) with a sidebar accent dot driven by `ProjectView::nav_dots` when `AttackFilter` is non-default.
Added `"Attacker profile changed"` to baseline diffs and added non-default attacker profile summaries and factor/scalability tables to the report cover, `Method` section (HTML/PDF), and XLSX `"Attacker profile"` sheet.
Default-filter reports and existing `report_configuration.rs` block offsets remain unchanged; the interactive Slint page body stays deferred as planned.
Look at:
- `crates/aegis-client/src/report/doc.rs:475-525` — `attacker_profile_table` shared between the report `Method` section and the XLSX `"Attacker profile"` sheet, handling zero-factor packs and all-allowed/all-excluded levels.
- `crates/aegis-client/src/report/doc.rs:1496-1521` — `section-method` emission and fallback `report-excluded` table when `!has_conditions && !has_assumptions`.
- `crates/aegis-shell/ui/app.slint:530-550` — `global Pages`, `nav-entries`, and `nav-dots` layout aligned with `Page::ALL` indices `0..=17`.
Risk: none — default-filter reports are unchanged (verified by `report_configuration.rs`), active profiles and baseline diffs are covered in `en` and `ja` across doc/HTML/PDF/XLSX (`attacker_profile.rs`), and sidebar dot state plus all 18 pages opening are verified in `nav_groups.rs` and `standalone.rs`.
View: git -C factory/worktrees/137-attacker-profile diff 8ed9e95e596798f0e8ddf12484fe068f4a5d8162..HEAD
## Findings
none
## Lesson
none
NEXT: human
