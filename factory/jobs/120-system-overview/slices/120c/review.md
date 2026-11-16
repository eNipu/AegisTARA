VERDICT: PASS
## For the owner
What changed: an attack step that goes through a component or interface of a different system now shows that system's name on its via chip ("GW · Gateway").
The step inspector has a new field, `through_system`, that holds the same name.
Five tests cover: another system, the tree's own system, a tree in no system, a project with no systems, and a setting from another system that caps a step.
Look at: crates/aegis-client/src/view.rs:866 (`step_other_system`: this is the rule for when a step "leaves" its tree's systems. When a tree belongs to several systems, the suffix only appears if the via system is none of them.)
crates/aegis-client/tests/combined.rs:369 (the "every view" test compares the full risk row with the row that the Brake filter keeps. It holds today because `filter_page` only removes rows and never recomputes them, so it is a regression guard and not proof of new behaviour.)
Risk: none found. The system lookup runs once per tree scene and once per inspector call, and only when the project has systems. It is a single pass over the project. A via system that has no name, or a system that is missing, gives no suffix and does not panic.
View: git -C factory/worktrees/120-system-overview diff 364578ce3943bfe6d22d720df6ec7d7f7c1d9d10..HEAD
## Findings
- none. All three "Done when" items are met. The suffix tests would fail without the change: they assert the exact chip text and `through_system`, and the negative cases assert that there is no suffix. The capped assertion and the row-equality assertion also passed before this slice. That is fine, because the prompt asked for them as guards.
## Lesson
none
NEXT: human
