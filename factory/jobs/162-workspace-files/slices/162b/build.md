Commit: a65dece Read workspace files back into a project (162b)
Files: crates/aegis-model/src/store/workspace.rs, src/error.rs, src/lib.rs, tests/workspace.rs, tests/workspace_properties.rs
Done when:
- cargo test --locked -p aegis-model --test workspace: 15 passed
- cargo test --locked -p aegis-model --test workspace_properties: 2 passed
- scripts/ci.sh crates/aegis-model: fmt, clippy, test all ok
- Source change outside tests/: 213 insertions, 3 deletions (under 400)
Notes:
- New `WorkspaceProblem` enum (Format, Version, Kind, Json) in error.rs, exported from lib.rs; used by `StoreError::WorkspaceEnvelope`.
- Extra behaviour beyond the prompt: a repeated path gives WorkspacePath; a file with a key not held by its kind gives WorkspaceEnvelope Json (so keys cannot overwrite another file's keys). Both are cheap placeholders for 162c.
- The writer drops empty lists but the snapshot requires assets, damage_scenarios, threat_scenarios, so the reader inserts empty ones when absent. Joined object is a serde_json Map (sorted keys); the snapshot reader does not care about key order and save() is byte-equal in all tests.
- Errors inside Project::load are returned unchanged (Decode positions refer to the rebuilt single-line snapshot, not to a file; no path attached). 162c may want to decide whether that is acceptable.
- Tree files are joined ordered by path (string order); the property tests pass with this.
- Moved the extra-ops setup in tests/workspace.rs into a helper `extra_ops()` shared by two tests.
## Rework 1
Commit: 893a4b6 Read tree files in number order and fill visual IDs from file names (162b rework)
- Tree files are now sorted by the number parsed from the file name (`VisualPrefix::Tree.parse_num` on the upper-cased stem), path as tie-break; unparsable names go last. A tree record without `visual_id` gets one from the file name. Writer (162a) untouched. Hand-written snapshots with trees out of visual order are reordered by file name order only when they come as workspace files; this is the same rule the writer uses.
- Repeated-path check is a `BTreeSet<&str>`.
- `wire::FORMAT_VERSION` replaces both literal `1`s.
- `file_order_does_not_matter` also compares `save()` bytes; new test `many_trees_keep_their_visual_ids` (105 trees, normal and reversed order, `save()` bytes).
- Project::load error-path question left for 162c.
- Checked: clippy ok, workspace tests 16 passed, workspace_properties 2 passed, scripts/ci.sh test and clippy ok; fmt fixed with cargo fmt after the ci run.

NEXT: reviewer
