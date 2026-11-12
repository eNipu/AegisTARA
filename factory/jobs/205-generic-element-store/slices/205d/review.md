VERDICT: PASS
## For the owner
What changed: a misuse case can now link to any number of threat scenarios, and one threat can be linked from many misuse cases, through two new undoable ops: link and unlink.
A threat scenario that a misuse case links cannot be deleted until the link is removed, and a misuse case with links cannot be deleted either. Each element can have at most 256 links.
Both ops are written to and read back from the op log as `{"op":"link_element","id":…,"target":{"threat":…}}`.
Look at: crates/aegis-model/src/project.rs:8070 (new last guard in `check_remove_threat`; this is the only rule that stops a dangling link, and asset removal is already blocked while the asset has threats); crates/aegis-model/src/element.rs:304-321 (`inbound` count: it is derived, kept up to date only by the link and unlink commits, and must be rebuilt when 205e loads a snapshot); crates/aegis-model/src/element.rs:496-519 (link checks, in the order the prompt gives).
Risk: low. Link targets are stored sorted, so output does not depend on order. The cap is checked in `validate`. Error messages contain only IDs, counts and the cap. No unwrap is used in src/. The new tests cover round trips, every reachable refusal, the cap boundary (256 accepted, the 257th refused), both removal guards, and the op log. Every `Op` variant still undoes and redoes. Known gap: `inbound` is not stored in snapshots yet (205e is planned to handle that).
View: git -C factory/worktrees/205-generic-element-store diff 4622528e1106b3f984cfa5b22199686c860305b8..HEAD
## Findings
- none. All seven "Done when" test items are present and assert the exact variant. Each test would fail if its guard were removed: the threat-removal test also checks the count drops from 2 to 1 after the first unlink. `LinkNotAllowed` cannot be reached through ops, so it is covered by the `allows_reads_the_kind_table` unit test, as the prompt said. The commits change provenance even when the link set itself does not change. That case only happens if `validate` is skipped, which matches the 205c pattern.
## Lesson
[planner] A derived index such as `ElementStore::inbound` that only the op commits update must be rebuilt in the snapshot loader. Make it an explicit "Done when" item in the load slice (205e), with a test that loads a snapshot and then deletes a linked threat, which must be refused.
NEXT: human
