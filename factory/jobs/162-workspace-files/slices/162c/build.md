Commit: 63bf31b Refuse bad workspace files and cap their count and size (162c)
Files: crates/aegis-model/src/store/workspace.rs, src/store/mod.rs, src/error.rs, src/lib.rs, tests/workspace_limits.rs (new)
Done when:
- `cargo test --locked -p aegis-model --test workspace_limits`: 13 passed
- `cargo test --locked -p aegis-model --test workspace --test workspace_properties`: 16 + 2 passed
- `scripts/ci.sh crates/aegis-model`: fmt, clippy, test all ok
- Size: 339 insertions, 54 deletions outside tests/ (net about 285, under 400)
Limits:
- MAX_WORKSPACE_FILES = 10_010 (10 fixed files + 10,000 trees). The prompt says to start from "the existing cap on attack trees per project", but there is none in the model; only the 64 MiB total bounds trees. 10,000 is my choice (a 2,000-threat project has about 2,000 trees). The doc comment says so.
- MAX_WORKSPACE_FILE_BYTES = 16 MiB (the biggest typical file is about 1 MB; 4 such files reach the total cap).
- Sum of sizes: existing MAX_PROJECT_FILE_BYTES (reports Limit::ProjectFileBytes).
- New Limit cases: WorkspaceFiles, WorkspaceFileBytes. Both consts exported from lib.rs.
Choices on the "Left for 162c" notes:
- Tree file with no number in the name (`tree.json`, `at.json`) and non-canonical numbers (`at5.json`, since `parse_num` wants two digits): refused with WorkspaceTreeName. Upper case is refused earlier by the path rules.
- WORKSPACE_VERSION (u32 = 1) is a new constant in workspace.rs, used by the writer and the reader. The snapshot still uses wire::FORMAT_VERSION for the joined snapshot only.
- Project::load errors: wrapped as StoreError::InWorkspace { path, source: Box<StoreError> }. Project::load is unchanged. The path is found as follows. Only ModelError values are placed.
  - DuplicateAttackTree(id) and DuplicateId(node id): the ID's text is searched in the tree files' raw bytes in load order; the second file holding it is named (the first if only one holds it).
  - UnknownAsset, UnknownDamageScenario, DuplicateThreatScenario: systems/main/threats.json.
  - Duplicate asset, damage, control, goal, claim, component, interface, data flow, trust boundary: the file of that snapshot key.
  - Everything else (Decode, UnknownKey, LimitExceeded, Cycle, other ModelError, UnknownThreatScenario, UnknownAttackTree and so on): aegis.json. These cannot be mapped because the inner error does not say which file.
- Pass 1 collects (path, bytes) after the three caps and the path and layout and duplicate checks, so nothing is parsed before the caps pass. The collection is bounded by MAX_WORKSPACE_FILES; the iterator is not drained (a test feeds an endless iterator).
Notes:
- Methodology: Project::load reports a key mismatch as MethodologyMismatch; that is mapped to WorkspaceMethodology { path: "aegis.json" }. The version is NOT checked: load accepts any methodology version on purpose (ADR 0009, open question 1), and the prompt says not to add a second check where load already decides. Say if the owner wants a version check.
- Path rules run before the layout match; all refusals are WorkspacePath. Repeated path is now WorkspaceDuplicatePath (was WorkspacePath).
- Unknown top-level key is now WorkspaceUnknownField (was WorkspaceEnvelope/Json). A tree record whose visual_id disagrees with the file name, a tree file with zero or two trees, or a non-object entry: WorkspaceTreeName.
- Error text: path in errors is now cut by both bytes (256) and escaped length, because `{path:?}` of 256 control characters would exceed 512 bytes. The 1 MB test covers path, field, and tree-name cases. WorkspacePath path bytes are still at most 256.
- Checks now run in two passes, so a bad path in a later file is reported before an envelope error in an earlier one.
- A refusal returns Err before any Project is returned.

## Rework 1
- Finding (path length never capped): `take_files` now refuses any path over 256 bytes (`MAX_PATH_IN_ERROR`) with `WorkspacePath` before `path_ok` and `lookup`, so `to_uppercase` never sees an oversized name.
- Test: `path_rules` gains a 260-byte path case. The 1 MB tree-name case in the error-size test now gets `WorkspacePath`; it still checks the 512-byte error limit.
- Ran fmt, clippy -D warnings and `cargo test -p aegis-model`: all pass. Change is 6 added lines; slice stays under 400 counted lines (non-test code only grew by 4).
NEXT: reviewer
