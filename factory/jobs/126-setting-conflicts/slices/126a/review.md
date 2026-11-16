VERDICT: PASS
## For the owner
What changed: Added `conflicts_with` to setting presets (model data, pack wire format, validation), a `ConflictingSettings` lint code that warns when both presets of a conflicting pair are applied to the same component or interface, and the matching Problems-page row.
What changed: Shipped packs stay byte-identical because the field is skipped when empty; the lint collapses two-sided declarations to one pair per target and is a warning, never a block.
What changed: Validation refuses unknown, repeated, self and over-cap conflict lists; `ItemIdx` gained `Ord` for the per-target aggregation map.
Look at: crates/aegis-model/src/project.rs:5676 — `lint_setting_conflicts` builds the per-target kind sets and emits one pair per target; check the early-return and sorted pair insertion.
Look at: crates/aegis-model/src/methodology/mod.rs:1631 — second pass resolves `conflicts_with` only after `by_key` is complete; indices sorted.
Look at: crates/aegis-model/src/lint.rs:246 — new `ConflictingSettings` variant carries `ItemRef` + two `ControlKind` indices and is a `Warning`.
Risk: none — gate ran ci.sh and all named test suites; size stays at 220 source lines.
View: git -C factory/worktrees/126-setting-conflicts diff 0182a0577c5ed1523da439ed45118c0e48e7ef83..HEAD
## Findings
none
## Lesson
none
NEXT: human