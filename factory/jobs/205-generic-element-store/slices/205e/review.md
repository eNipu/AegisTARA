VERDICT: PASS
## For the owner
What changed: misuse cases are now saved, in the project snapshot (a new `elements` key, left out when empty) and in their own workspace file `systems/main/misuse.json`.
Loading reads them strictly. It checks size caps first, then adds each case through the normal model operations, so every check from 205b to 205d runs again on load.
A loaded project still refuses to delete a threat that a misuse case links, with the correct count. Projects without misuse cases save exactly the same bytes as before.
Look at: crates/aegis-model/src/store/snapshot.rs:1194-1208 and 1266-1295. This is the load loop. It checks the visual IDs, then rebuilds the link counts by replaying each stored link as `LinkElement`. It does not write `links` or `inbound` directly.
crates/aegis-model/tests/ops_properties.rs:333-339. Element ops are left out of the diff property test because `Project::diff` does not cover elements until 205f. 205f must remove this filter.
crates/aegis-model/src/store/workspace.rs:291-300. When a load error names an element ID or a threat ID, the error now names the element file that contains that ID. If a tree also refers to the same missing threat, the error may name misuse.json instead of the tree file. Both files are wrong in that case, so this is acceptable.
Risk: low. Untrusted input is capped while decoding: elements per kind, attributes and links per element, and kind keys, which must be known and unique, so the map can have at most `KINDS.len()` entries. Tests cover each cap just under and just over. Every refusal is tested with its exact error. Two tests prove the link count on load: `loaded_snapshot_refuses_to_remove_a_linked_threat` and `loaded_workspace_refuses_to_remove_a_linked_threat`, which check 2, then 1, then the delete is allowed. They would fail if loading filled `links` without counting them or counted them twice. Round trips compare `save()` bytes for 150 misuse cases added in reverse ID order (L06). No new dependency, and no unbounded input in error text.
View: git -C factory/worktrees/205-generic-element-store diff 1bee844fc95c1b8eb31f96d629d22a321e0d280e..HEAD
## Findings
- none. All six "Done when" items are met and each has a test that would fail without the change. The diff-test filter in ops_properties.rs is a test-only deviation. It was reported in build.md, and the fix belongs to 205f, which covers diff.rs.
## Lesson
[planner] When a slice adds a generator arm for new ops, check that every property using the generator (diff, inverse, round trip) already handles those ops, or the builder has to weaken a test. Here the diff test was filtered until 205f.
NEXT: human
