VERDICT: PASS
## For the owner
What changed: an element (misuse case) can now carry extra attributes (a level, a choice or a short text) under a key.
One new op sets or clears one attribute, undoes exactly, and is written to and read back from the op log.
An element cannot be removed while it still has attributes; the user clears them first.
Look at: crates/aegis-model/src/element.rs:541 (`commit_set_element_attribute`): the one place that inserts, replaces or removes in the sorted list and builds the undo op. All four cases (set new, replace, clear, clear unset) are tested.
crates/aegis-model/src/element.rs:446 (`check_remove_element`): the new removal refusal. It keeps undo of a removal exact, because the undo is a plain add with no attributes.
crates/aegis-model/src/store/op_log.rs:572 (`within_caps`): text values are capped at 1024 bytes, which is 256 chars x 4 bytes, so any text the model accepts can always be written.
Risk: low. Setting an attribute also replaces the element's provenance, as the prompt asks. The undo restores the old provenance, and the round-trip test checks this with `==` (Element derives PartialEq over attributes and provenance). Error text names only the element ID and the counts, never a key or a value.
View: git -C factory/worktrees/205-generic-element-store diff 88b9124b42314a8bae4618a7f423a14496885f57..HEAD
## Findings
- none. Every "Done when" item is met. Each refusal test uses an input that only that guard refuses, and asserts the exact variant and an unchanged project. The round-trip and removal tests would fail without the change. `value` is required on the wire (`Option::deserialize` with no `default`), and the test drops it and gets `Decode`. No new dependency, no hash map keyed by input, no `unwrap` or indexing in `src/`.
## Lesson
none
NEXT: human
