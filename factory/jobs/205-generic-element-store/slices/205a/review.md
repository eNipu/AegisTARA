VERDICT: PASS
## For the owner
What changed: Adds a public `ElementId` (UUIDv7, same shape as `SuggestionId`) and a kernel table of generic element kinds with one row, the misuse case (prefix `MU`).
Also adds the link and attribute value types with their caps, and moves the visual-number parsing into a shared crate function with no behaviour change.
Nothing uses these yet. The store, ops and file formats come in later slices.
Look at: crates/aegis-model/src/element.rs:205-231 (the syntax checks, unused until 205c and covered only by unit tests); crates/aegis-model/src/id.rs:1109-1126 (the `parse_num` refactor, which must stay identical); crates/aegis-model/src/element.rs:186 (`visual_id` uses `{num:02}`, which matches the 2-digit minimum in `parse_visual_num`).
Risk: none. No new dependency, no format change, no panic paths (no unwrap, indexing or `as` casts). `is_attribute_text` caps its char count with `take(MAX+1)`, so it is O(cap) on hostile input. The gate ran fmt, clippy, the tests and the size check, and all passed.
View: git -C factory/worktrees/205-generic-element-store diff b4a40d95c7ea670e4a631335194e36d9ba200294..HEAD
## Findings
- None blocking. Minor: `is_key` checks `s.len() <= 64` after the first-char check. This is correct and cheap, and the tests cover 64 and 65. `is_attribute_text` is only called with length-capped input, so no change is needed.
## Lesson
none
NEXT: human
