VERDICT: PASS
## For the owner
What changed: in the attack tree outline text, a step line can now end with ` | through: C00 | type: debug-access`.
The reader takes these two values off the end of the name and reports them for each step. The writer puts them back.
A name that would otherwise be misread (for example `A | type: x`) is written with ` | through: - | type: -` so it reads back whole. Nothing uses the values in the project yet.
Look at:
- crates/aegis-client/src/outline.rs:252 `read_tail`: the rule that decides what is name and what is a value. Every outline line goes through it.
- crates/aegis-client/src/outline.rs:173 `tail_of`: decides when the writer adds the ` | through: - | type: -` guard. This is what keeps odd names exact on a round trip.
- crates/aegis-client/src/outline.rs:461: the shared-step rule. A later line with no tail is fine. A later line with a tail must match the first line exactly.
Risk: low for names. The tests round-trip 7 odd names against 4 value pairs, plus an odd gate name. I also checked by hand names ending in ` |`, `A | type: x | B` and a guarded name on a shared step. All of them read back the same.
Values are different: the writer does not check them. A value of `-`, an empty value, or one that contains ` | ` or a line break would not read back the same, and a line break would add a line to the outline. Today nothing passes values in (`to_outline` passes none). The next slice feeds project IDs and attack type keys into `to_outline_with`, so it must refuse or check such values (the builder notes say the same).
View: git -C factory/worktrees/122d-through-in-outline diff 60e6f4a37ac6d1f05105d5cc4884230ffa30c9f2..HEAD
## Findings
- none. All three Done-when items are met, and the tests in tests/outline_notes.rs would fail without the change: the examples, the error variants with their line numbers, notes order and root, gates, shared steps, the 256/257 limit and the round trip. No panic can be reached: the slice index sits after the ASCII ` | `, and `tails` is sized to `tree.len()` as `seen` was before. No user text goes into an error message. No new dependency. Scope is limited to the two allowed files.
- Minor, no action needed: with `A | type: | type: b`, the duplicate-key check runs before the empty-value check, so it gives `DuplicateNote`. The prompt allows either; the builder noted it.
## Lesson
[planner] When a writer takes free values (IDs, keys) through a closure into a line-based text format, the slice that supplies them must reject values that are empty, `-`, or contain the separator or a newline. Otherwise round-trip tests on names alone miss it.
NEXT: human
