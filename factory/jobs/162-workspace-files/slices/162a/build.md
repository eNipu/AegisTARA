Commit: 0dacc05 Write a project as workspace files (162a)
Files: crates/aegis-model/src/lib.rs, src/store/mod.rs, src/store/snapshot.rs, src/store/workspace.rs (new), tests/workspace.rs (new)
Done when:
- 1: cargo test --locked -p aegis-model --test workspace -> 6 passed
- 2: scripts/ci.sh crates/aegis-model -> fmt, clippy, test all passed
- 3: 175 lines added/changed outside tests/ (under 400)
Notes:
- Deviation: the snapshot always writes `assets`, `damage_scenarios`, `threat_scenarios` (empty arrays when empty). Taken literally, an empty project would then write 4 files, contradicting the `empty_project_writes_only_the_workspace_file` test. So a file is skipped when all its keys hold empty arrays. The reader (next slice) must treat a missing file as empty.
- `Project::save` was refactored to share a private `with_snapshot_view` helper; its output is unchanged (all existing tests pass).
- serde_json has no `preserve_order` here, so keys inside each file's body come out in alphabetical order, not snapshot order. The three envelope keys are first. Output is deterministic.
- Tree file names come from `attack_tree_visual_id` (the snapshot omits the visual ID when it equals the position, so it cannot be read from the value). `snapshot_value` returns them alongside the value.
- Error for an unplaced key or a missing visual ID is `StoreError::UnencodableOp` (no better variant exists; adding one was out of scope).
- `layout_covers_every_snapshot_key`: `record_every_op` ends with a torn-down project and omits interfaces, data flows, trust boundaries, review marks, descriptions and languages. The test checks after every recorded op, then adds a second builtin-methodology project with those elements, and requires every key to be seen.
NEXT: reviewer
