VERDICT: PASS
## For the owner
What changed: Adds a typed `Condition` element (name, description, present, provenance) with `ConditionId`/`ConditionIdx`, `CF` visual prefix and an arena on `Project`. Adds four ops (`AddCondition`, `RenameCondition`, `SetConditionPresent`, `RemoveCondition`) with exact inverses, guards and error variants, plus wire/op-log codecs and a `ProjectDiff::conditions()` accessor. Extends `Project::eq`, `Project::diff`, the every-op walks and the inverse/diff/guard tests.
Look at: `factory/worktrees/133-conditions-model/crates/aegis-model/src/project.rs:9203` (`check_add_condition`: duplicate/unknown, blank/over-256 name, over-cap description via shared `DescriptionTooLong`, provenance, 256 cap + slot limit) and `project.rs:10848` (`commit_add`/`commit_remove_condition`: remove inverse restores name, description, present and provenance through the Arena's ID-keyed visual reuse). Also `project.rs:11573` and `src/diff.rs:6064` (public-ID zip in `eq` and field-based `diff_arena` closure, L06/L29) and `src/id.rs:1106` (`CF` prefix distinct from `A/D/T/.../P/C`).
Risk: `RemoveCondition` is intentionally unguarded here; the `ConditionInUse` guard is 133b per plan, and no links exist yet, so there is no present-day way to orphan a condition. Checked that the inverse is exact (`tests/conditions.rs:79`), that guards refuse only their own input (`:142`), that the 256/257 boundary holds (`:249`), that visual number `CF100` survives remove+readd via public ID (`:416`), and that decode caps come from bounded `Name`/`DescriptionText` and `within_caps`. No v1 fixture or byte-layout files changed; `store` and `undo_every_op` tests are green in gate.md.
View: git -C factory/worktrees/133-conditions-model diff b8d70b9..HEAD
## Findings
- none
## Lesson
none
NEXT: human
