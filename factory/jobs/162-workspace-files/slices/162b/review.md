VERDICT: PASS
## For the owner
What changed: `aegis-model` can read a list of workspace files back into a project. It checks each file's path, `format`, `version` and `kind`, then hands one rebuilt snapshot to `Project::load`.
The rework puts tree files in order by the number in the file name, so `at100` now comes after `at99`. A tree record with no `visual_id` takes its ID from its file name. The 162a writer is unchanged.
It also checks repeated paths with a `BTreeSet`, replaces the version literal with `wire::FORMAT_VERSION`, and adds a 105-tree round trip, in normal and reversed order, that compares `save()` bytes.
Look at: crates/aegis-model/src/store/workspace.rs:254-270 — the fix itself. It works because the writer names tree files after their visual ID (workspace.rs:144, `visual.to_lowercase()`), so the file name and the ID cannot disagree.
crates/aegis-model/src/store/workspace.rs:242-243 — the check on the workspace envelope's `version` now uses the snapshot's `FORMAT_VERSION`. The writer still writes a literal `1` (workspace.rs:61). They are two formats tied to one constant. If the snapshot version is raised, every round-trip test fails at once, so this cannot go wrong without anyone noticing. Still, a separate `WORKSPACE_VERSION` constant used by both the writer and the reader would be cleaner. This can go into 162c.
crates/aegis-model/src/store/workspace.rs:295-298 — still open, as the owner decided: errors from inside `Project::load` carry no file path. 162c decides.
Risk: low. The 105-tree test (tests/workspace.rs `many_trees_keep_their_visual_ids`) compares `save()` bytes, and it would fail without this change: the first review reproduced AT11 turning into AT12 at 101 trees. Hostile input cannot cause silent harm here. Several trees in one file get the same filled-in ID, and two names that map to the same number (`at05`/`AT05`) give the same ID too. Both end with `Project::load` refusing the duplicate IDs. A tree file name with no number, such as `at5.json`, is sorted last and gets no filled-in ID. 162c should make that a refusal.
View: git -C factory/worktrees/162-workspace-files diff 0dacc058efdc19f14d419c623ed45e033d34e946..HEAD
## Findings
none blocking. All four findings from the first review are fixed as the owner asked in rework.md. No new dependency, no `unwrap` in `src/` (`tree_stem` uses `unwrap_or`), and no hash map keyed by input.
## Lesson
[builder] When asked to replace a literal with a constant, change only the use that was named: here, the workspace envelope's version is not the snapshot's version.
NEXT: human
