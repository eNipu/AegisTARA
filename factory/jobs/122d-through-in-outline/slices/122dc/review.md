VERDICT: PASS
## For the owner
What changed: A step of a library item can now name an attack type by key (for example `debug-access`). Inserting the item sets that type on the copied step as a normal, changeable value, and the tree's description says on how many steps this was done.
The library preview shows the key as `{type: key}`. The demo pack has the type on three kinds of step (reflash, open debug port, dump key storage) and a new hash.
A key the project's methodology does not know is skipped silently; a step that is linked to an existing step is left alone; no review mark is set.
Look at:
- crates/aegis-client/src/library.rs:219 — `Builder::attack_type`: the only place that writes the value. It runs on every insert, with or without ratings, and skips unknown keys without telling the user. That silence is what the prompt asked for; decide if you are happy that a pack written for another methodology loses its suggestions without a word.
- crates/aegis-client/src/library.rs:284 — the origin note sentence the customer will read in the tree's description ("Suggested attack types were filled in on N step(s); check them in the step inspector."). The wording is yours to judge.
- tools/aegis-ai/src/dataset.rs:887 — `bind_node_slots` writes `attack_type: None`, so the AI training data drops the suggestion from library items. Done as the prompt said; it is a one-word change later if the training data should carry it.
Risk: low. The pack field is omitted when empty, so the hash of existing packs does not change (a test proves the text `attack_type` is not written). The key from a pack file is capped at 64 characters and refused when blank or on a gate, each with a fixed error text and an exact-error test. The insert stays one batch: one undo takes back the tree, the attack type and the note (tested). The description length check under a gate or in place of a step now runs on the final text including the new sentence. The undo test compares with `Project ==` (lesson L06), which is enough here because the project before the insert has no tree at all.
View: git -C factory/worktrees/122d-through-in-outline diff efcc00aa143c1543f7943c56079f10968bc26561..HEAD
## Findings
- none that need a change.
- Checked, each "Done when" item has a test that fails without the change: fill and count (`a_suggested_attack_type_is_filled_in_counted_and_undone_together`), no sentence without a suggestion, unknown key skipped, shared step untouched and value clearable, preview text, pack round trip, three refusals, field not written when empty, demo pack carries the field.
- Scope: only the listed files changed; no new dependency, Op, Edit, option or format version. The demo pack diff is the hash plus the three replacements (five places, because two of the steps occur twice).
## Lesson
none
NEXT: human
