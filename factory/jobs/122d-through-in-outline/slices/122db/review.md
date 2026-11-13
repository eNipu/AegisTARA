VERDICT: PASS
## For the owner
What changed: The outline editor now shows each step's "through" item (C00, IF00) and attack type after its name.
Applying the outline sets, changes or clears both values in the same undo step as the tree change; a wrong ID or key is refused with the line number, and nothing is applied.
The outline writer now leaves out any value that would not read back exactly, so it can never add a line or break the text.
Look at: crates/aegis-client/src/commands.rs:958-994 (`note_ops`): compares each step's wanted values with its current ones and adds an op only when they differ. A plain line now clears both values. That is intended ("the text decides"), but it changes what users see.
crates/aegis-client/src/outline.rs:250-287 (`resolve_notes`): reads the user's text. It runs before any op is built, so an error applies nothing. The error messages contain only a line number, never the user's text.
crates/aegis-client/src/outline.rs:186-197 (`is_note_value` at the top of `tail_of`): the guard against values that would inject extra lines or tails into the outline.
Risk: Low. Someone who opens an outline that was written before this change and applies a plain line clears that step's values. Undo restores them (tested in a_plain_text_clears_both_values_and_one_undo_restores_them). The editor now always shows the values, so a normal edit keeps them (round-trip tests give no ops, including one for every attack type key of the methodology).
View: git -C factory/worktrees/122d-through-in-outline diff 5c0433a55f240f0b65f01009696aa121927444e0..HEAD
## Findings
- none. Every "Done when" item is met, and each has a test that would fail without the change: set, clear, `-`, new step, moved step, unknown item (C99, T00, banana), unknown type (line 3), removed component, unsafe writer values, and every attack type written and read back. The error tests check the exact variant and line (L09). Tests compare the annotated outline and node count, not `Project ==`, so L06 does not apply. The ID lookup is a project-built map, read with uppercased user text that the parser has already capped. There is no new dependency and nothing outside the scope.
## Lesson
none
NEXT: human
