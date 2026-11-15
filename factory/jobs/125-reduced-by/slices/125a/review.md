VERDICT: PASS
## For the owner
What changed: one new read-only function, `Project::deciding_step`. It names the attack step that decides a tree's residual feasibility.
It also returns the security settings that actually lower that step, lowest first, so the "Reduced by" column (CFG-07) has one shared definition.
No number, op or file format changed. Six new tests and one doc test cover it.
Look at:
- crates/aegis-model/src/project.rs:3401 — the only guard against an evaluation from the wrong tree is a node-count check. An evaluation of a different tree with the same node count, or an old evaluation from before an edit, is still accepted. The prompt asks for exactly this, and today's callers evaluate and read at once, but later callers must keep it that way.
- crates/aegis-model/src/project.rs:3410 — the step's residual is recomputed from the project, not read from the evaluation passed in. The two agree when the evaluation is current.
- crates/aegis-model/src/project.rs:3414 — strict `<`. A setting equal to the step's own rating is not listed as "reduced by". The test `a_ceiling_at_or_above_the_rating_is_not_listed` covers both equal and above.
Risk: low. It only reads. It cannot panic (it uses `?`/`Option`, and `why_path` is capped at `len()`). The output is in a fixed order: `step_ceilings` order, ties broken by control id, and a test applies the controls in both orders. No untrusted input, no new dependency, no names in the type.
View: git -C factory/worktrees/125-reduced-by diff 51bc2ad769f84b727fcb5eadbe2e25cc75b6bd4f..HEAD
## Findings
- none. All three Done-when items are met. Each test would fail without the change: the filter, which node is chosen, the source target and the order are all asserted. The doc test uses `physical-tampering` instead of `debug-access`, because it copies the `StepCeiling` doc test as the prompt said. That is harmless.
## Lesson
none
NEXT: human
