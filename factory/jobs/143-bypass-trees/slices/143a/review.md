VERDICT: PASS
## For the owner
What changed:
Added `SecurityControl::bypass: Option<TreeIdx>` and `Op::SetControlBypass { control, tree, provenance }` with full fragment-role, cycle, depth, and removal guards.
Updated `same_control` (used by `Project::eq` and `Project::diff`) to compare bypass trees by public `TreeId` and checked bypass invariants in `check_security_control_invariants`.
Wired `bypass` through wire types, op logs, snapshots (omitted when `None`, replayed after trees/fragments), workspace file blame (`shared/controls.json`), property generators, and integration tests.
Look at:
- `crates/aegis-model/src/project.rs:1610-1729` — `collect_fragment_deps` and `check_fragment_graph` fold `UseFragment` and control-bypass edges across all `ControlStatus` values and candidate edits so status switches cannot introduce cycles or exceed `MAX_FRAGMENT_DEPTH`.
- `crates/aegis-model/src/store/snapshot.rs:1718-1726` — `pending_bypasses` replay after `attack_trees` and `pending_fragments` load, preserving each control's provenance while running live role/cycle/depth checks.
- `crates/aegis-model/src/store/workspace.rs:361-383` — `blame` routes bypass-related `UnknownAttackTree`, `FragmentRole`, `FragmentCycle`, `FragmentDepth`, and non-tree `UnknownSecurityControl` load errors to `"shared/controls.json"`.
Risk: A `SecurityControl` with an unreferenced `bypass` still blocks `RemoveAttackTree` (`FragmentInUse`) and `RemoveSecurityControl` (`SecurityControlHasBypass`), and `FragmentDepth` blame assumes `"bypass"` in `shared/controls.json` implicates controls when depth fails; checked by unit/integration tests in `crates/aegis-model/tests/store.rs`, `workspace.rs`, and `ops_properties.rs` (4096 proptest sequences cover all 83/80 ops and undo/diff invariants).
View: git -C factory/worktrees/143-bypass-trees diff 2c8bc117eff8332b935dcd4b066313bbd6fa6c5c..HEAD
## Findings
none
## Lesson
none
NEXT: human
