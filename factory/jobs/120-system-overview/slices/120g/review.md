VERDICT: PASS
## For the owner
What changed: the Versions page now lists only the changes in the systems you are looking at (for example only Brake).
The summary counts only those lines and adds "N more outside this view." when changes in other systems are left out.
Picking all systems gives exactly the same comparison as before; the report and older callers are not affected.
Look at:
- crates/aegis-client/src/pages.rs:3260 — the rule that decides if a line shows. Added and changed elements are placed by the current project, removed ones by the baseline. Elements that belong to no system always show.
- crates/aegis-client/src/pages.rs:3321 — a component that moved between systems shows in the view of the old system and in the view of the new one.
- crates/aegis-client/src/pages.rs:3472 — the wording when every change is outside the view ("Nothing changed in this view since this baseline. N more outside this view.").
Risk: low. The view comes from the current project, so a system deleted since the baseline cannot be picked. Its "Removed system" line shows only under all systems, but it is still counted in "N more outside this view." The all-systems path is checked against the old `comparison` field by field (crates/aegis-client/tests/combined.rs, `all_systems_is_the_plain_comparison`).
View: git -C factory/worktrees/120-system-overview diff c2202b5ded72ae14a8d62749ea4114971bc9dfb7..HEAD
## Findings
- none. All three Done-when items are met (gate PASS). Each of the 8 new tests in `mod versions` (combined.rs:1019) would fail without the change: the per-kind ADAS absence, the Removed threat placed by the baseline, the moved component (Brake, ADAS, not Gateway), the system rename, one Risk line out of two, the summary text and `scope`, and the summary when nothing is shown. `set_of` ignores unknown IDs and `SystemUse::default()` is safe on the all path. The `entity!` macro refactor keeps the line texts and their order, which `all_systems_is_the_plain_comparison` confirms.
## Lesson
none
NEXT: human
