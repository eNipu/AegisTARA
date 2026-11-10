VERDICT: PASS
## For the owner
What changed: A project can now be turned into a list of small JSON files (one per kind of element, one per attack tree), in memory only.
It reuses the existing snapshot serializer, so the data is the same as in the single-file save; `Project::save` output is unchanged.
Six new tests cover layout, determinism, small diffs, the empty project, tree file names and the file header.
Look at:
- crates/aegis-model/src/store/workspace.rs:116-128 — empty-array rule: a file is skipped when all its keys are empty lists. This differs from the prompt's literal wording ("present in the snapshot"). The builder had to do it, because the snapshot always writes `assets`, `damage_scenarios` and `threat_scenarios`. The reader slice must treat a missing file as empty.
- crates/aegis-model/src/store/snapshot.rs:750-773 — `save` now goes through a shared helper. Existing tests pass, so the output is unchanged.
- crates/aegis-model/src/store/workspace.rs:129-143 — a tree file name comes from the visual ID. The snapshot leaves out a tree's visual ID when it equals its position. In a one-tree file the position is always 0, so the next slice (the reader) must take the ID from the file name and not from the position.
Risk: Low. Nothing touches the disk, and there is no new dependency and no panic path in `src/`. Tree file names come from a lower-cased visual ID such as `at01`, which is not free text. An unknown snapshot key or a missing visual ID gives an error; it is never dropped. Keys inside each file come out in alphabetical order (not snapshot order), but the output is deterministic, and the three header keys come first.
View: git -C factory/worktrees/162-workspace-files diff 534c7236b69d7ac33d1665a3908fdb51dd8b827a..HEAD
## Findings
- none blocking. Small notes: the error variant `UnencodableOp` is a loose fit for "snapshot has a key the layout does not place". Consider a clearer variant later.
- The `rename` test would fail without the change, because the file list would not exist. The `layout_covers_every_snapshot_key` test checks every key, and it also checks that each key is seen at least once.
## Lesson
[planner] The prompt's "write a file only when its key is present" ignored that the snapshot always writes some empty lists. Check what the real serializer emits before specifying presence rules. Also say how the reader gets a tree's visual ID back.
NEXT: human
