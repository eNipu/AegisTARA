VERDICT: PASS
## For the owner
What changed: recovery of an unfinished save now only replaces or removes files that belong to the project's own file set, so a planted journal can no longer touch `.git/` or other files.
One model rule (`WorkspaceFile::is_layout_path`) decides which names belong, and both the folder reader and the journal check use it.
In the workbench, opening the folder that is already open does nothing, and Save As into it is a plain Save. Before this change both failed with "Locked".
Look at: crates/aegis-client/src/folder.rs:526, where the journal check now also needs a layout path. This is the security fix.
crates/aegis-client/tests/folder_recovery.rs:229, where an existing link test was changed: it now links `systems/main/trees` so that only the link check can refuse it. It still expects `Symlink`.
crates/aegis-workbench/src/app/files.rs:53 and :123, the two `holds` early returns. `holds` is false for a missing path, so Save As into a new folder still creates it.
Risk: low. The journal could already only name `.json` paths inside the folder. Now it can name only project files. This was checked by `a_journal_outside_the_file_set_touches_nothing` and four more entries in the hostile list. The builder removed each guard on purpose and the matching tests failed. The gap where a symbolic link is swapped in between the check and the write remains, and ADR 0071 records it.
View: git -C factory/worktrees/164e-workspace-gaps diff 441912550ffdc8e41a32a06c2c893a3389a9f7dc..HEAD
## Findings
- none. All five Done-when items are met:
  - `layout_paths_are_exactly_the_written_ones` collects every path the project writes, checks the set is not empty (it must hold `aegis.json`, `shared/damage.json`, a tree file and an element kind file), and checks the full bad-path list.
  - `holds` covers `..` and symbolic links.
  - Both workbench tests would fail with "Locked" without the guards.
  - `from_workspace_files` behaves the same as before; `lookup` now runs twice, which costs nothing that matters.
## Lesson
[builder] When a new check makes an older test fail at the wrong place, change the older test's input so it still reaches the check it was written for, and say so in build.md. 164ea did this right with the link test.
NEXT: human
