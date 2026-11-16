VERDICT: PASS
## For the owner
What changed: A top-level component can now be put in one system, moved to another, or taken out, with one new op (`SetComponentSystem`) that undoes exactly.
A child component cannot have a system; a component in a system cannot get a parent or be removed; a system that still holds components cannot be removed.
The system is saved in the project file, the op log and the workspace files; old files load and save with no `system` key.

Look at:
- `crates/aegis-model/src/project.rs:8877` (`check_set_component_system`): the order of the checks, and that the blank-actor check runs on every path, also for `None`.
- `crates/aegis-model/src/store/snapshot.rs:1226`: systems are put on components only after the elements are loaded, so a file naming a missing system or a system on a child fails with the model error.
- `crates/aegis-model/src/element.rs:578`: removing a system counts its components in one pass over all components (O(components)); fine at today's sizes.

Risk:
- A file that uses systems cannot be opened by an older build: the snapshot version was not bumped (the prompt forbids it) and `ComponentWire` refuses unknown fields, so the old build reports an unknown field, not "made by a newer version". Files without systems are byte-identical to before. Decide whether that message is acceptable before this ships to a customer who has an older build.
- In the client, the new op has no entry in `crates/aegis-client/src/undo_label.rs`, so it falls into the wildcard arms: the undo label reads "change project" and undo does not select the component. That file was outside this slice's scope; a later slice that adds the screen must add both arms (lines 271 and 409).
- Inverse exactness was checked by the systems tests (set, move, clear), the ops property test (op 65 is generated and must succeed) and the client undo/redo walk.

View: git -C factory/worktrees/119-systems diff 7232db9690e387678da3b619ba2beea17c20597a..HEAD
## Findings
No defects. Notes, not blocking:
- `crates/aegis-model/tests/systems.rs:347` — the v1 fixture test checks that save, load, save gives the same bytes and that there is no `"system"` key; it does not compare with the fixture's own bytes. The no-key assertion does fail if `skip_serializing_if` is removed, and "old files load unchanged" is covered by the existing store tests in Done-when 3, so the item is met.
- Every guard test uses an input only that guard refuses and asserts the exact variant and ID; each would fail if its guard were removed. The round-trip test compares `save()` bytes (L06); the 100-plus part of L06 does not apply, as systems are capped at 64 and component file names did not change.
- Scope: nothing beyond the prompt; `lib.rs` untouched; 208 counted lines against the 400 cap.
## Lesson
[planner] A slice that adds an op must list `crates/aegis-client/src/undo_label.rs` in its allowed files (or name the later slice that adds the label and selection arms); its wildcard arms hide a missing op from the compiler.
NEXT: human
