VERDICT: PASS
## For the owner
What changed: the workspace reader now caps the number of files (10,010), the size of each file (16 MiB), the total size (64 MiB) and the length of each path (256 bytes). It checks all of these, and every path rule, before it reads any JSON.
It refuses a repeated path, an unknown field, a badly named tree file and a different methodology. Each has its own error, and the error names the file.
When the model check fails, the error says which file to open: the second tree file for a repeated ID, `systems/main/threats.json` for a threat whose asset is missing, and `aegis.json` when the file can't be worked out.
Look at:
- crates/aegis-model/src/store/workspace.rs:286 (`take_files`): the rework. A path over 256 bytes is now refused with `WorkspacePath` before `path_ok` and `lookup`, so the tree-name code no longer copies a name of any length. The fix is correct. The new test case (`shared/` + 260 letters, tests/workspace_limits.rs:161) would be refused even without the fix, because it is not a known file name. The case that really needs the fix is the 1 MB `systems/main/trees/at00…` path in the error-size test (tests/workspace_limits.rs:383). That test only checks that the error text is short, not which error comes back. This is a weak test, not a defect.
- crates/aegis-model/src/store/workspace.rs:224 (`tree_file_with`): to find the file behind a repeated tree or node ID, it searches for the ID's text in the raw bytes. If that text also appears somewhere else in a tree file, the error names the wrong file. The file set is still refused; only the file named is wrong.
- crates/aegis-model/src/store/workspace.rs:429: only a different methodology key is refused as `WorkspaceMethodology`. A different methodology version is accepted, because `Project::load` accepts any version on purpose (ADR 0009). The prompt asked for key or version. **Owner decides:** should a version mismatch be refused when reading a workspace?
- crates/aegis-model/src/store/mod.rs:163: `MAX_WORKSPACE_FILES` = 10 + 10,000. The prompt said to base this on the cap on trees per project, but no such cap exists, so the builder chose 10,000 trees. **Owner decides:** is 10,000 trees enough?
Risk: low. The caps are tested just under and just over. An endless file list is stopped at the count cap. The error text is tested to stay at most 512 bytes when paths and fields are 1 MB long. The earlier finding (no cap on path length) is fixed.
View: git -C factory/worktrees/162-workspace-files diff 893a4b660c0e2e6ebb34544c631ac2a418af7b54..HEAD
## Findings
none (the weak path-length test above is a note, not a defect)
## Lesson
[builder] A test for a new guard must use an input that only that guard refuses. A bad path that a later check also refuses passes even without the guard.
NEXT: human
