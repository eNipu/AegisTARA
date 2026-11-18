VERDICT: PASS
## For the owner
What changed: new module `crates/aegis-client/src/folder.rs` opens a workspace folder into a `Project`, creates a new one, converts an old folder beside its files, and saves by writing only changed files (stage `.aegis-new`, sync, rename) and deleting only files it read or wrote.
Reads of an untrusted folder are bounded: symlinks refused, depth ≤ 6, per-file 16 MiB, ≤ 10,012 files, ≤ 64 MiB total, all checked from metadata before any read, and again while reading.
14 tests in `crates/aegis-client/tests/folder.rs` cover round-trip, minimal writes, tree deletion, locking, create/convert rules and every refusal with its exact variant.
Look at:
- crates/aegis-client/src/folder.rs:239-295 — `save`: the only code that writes or deletes on disk; deletes only keys of `written`, which come from `to_workspace_files` or from paths the model accepted on open.
- crates/aegis-client/src/folder.rs:369-451 — `collect`/`walk`: the untrusted-folder guard (symlinks via `symlink_metadata`, depth, the three caps checked during the walk so a huge folder stops early).
- crates/aegis-client/src/folder.rs:143-146 — deviation from the prompt: `aegis.json` is checked before the lock is taken, so a wrongly picked folder does not get a `.aegis.lock`. Same results; I agree with it.
Risk: low. Check-then-use gaps remain: a file or a parent folder (`systems`, `shared`) swapped for a symlink after `collect` (open) or after open (save, via `create_dir_all`/`rename`) would be followed; std has no `O_NOFOLLOW`. Only someone who can already write the user's folder can do that. A save that fails half way leaves some files new and some old until the next save; the crash-safe journal is slice 164b. Newly created subfolders are not fsynced in their parent (only folders with a rename/removal are), also for 164b. Checked by reading every changed line and the tests: each refusal test would fail if its guard were removed (symlink would be skipped and open would succeed; deep file would become `WorkspacePath`), and `only_changed_files_are_written` would fail if save rewrote everything.
View: git -C factory/worktrees/164-standalone-workspace diff 38568effcd1bf035c6dcae6aafe5f69ffef44fad..HEAD
## Findings
none
## Lesson
none
NEXT: human
