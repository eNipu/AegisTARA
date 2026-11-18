Commit: 69b8b21 Journal workspace saves and recover an interrupted save on open (164b)

Files:
- crates/aegis-client/src/folder.rs (+270 / -24; cap 300)
- crates/aegis-client/tests/folder_recovery.rs (new)
- docs/adr/0071-workspace-folder-on-disk.md (new)
- docs/architecture.md (one row in the aegis-client module table)

Done when:
1. `cargo test --locked -p aegis-client --test folder_recovery`: ok, 6 passed (stopped saves open as old / new, nothing left over)
2. same command: hostile, link and oversized journal tests pass (victim and `shared/damage.json` untouched)
3. `cargo test --locked -p aegis-client --test folder`: ok, 14 passed
4. `grep -n "folder.rs" docs/architecture.md`: line 1681 lists `folder.rs` (ADR 0071)
Also: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-client`: fmt, clippy, test all ok.

Notes:
- `open` order: the prompt says lock, recover, then check `aegis.json`. I kept one cheap check before the lock: if there is neither `aegis.json` nor `.aegis-save.json`, return `NotAWorkspace` without creating `.aegis.lock`, so picking a wrong folder leaves no lock file in it. With either present: lock, recover, then check `aegis.json` again (a cut-off first save of `create` is still finished).
- Hostile journal test: each bad path follows a valid entry (`shared/damage.json`, in `remove`, or in `replace` with a planted `shared/damage.json.aegis-new`), so acting on entries before all are checked would change `damage.json` and fail. Each case runs in its own base folder with the workspace in `base/ws`, so `../victim.json` points at `base/victim.json`, not the shared temp dir. Mutation check: making `valid_journal_path` accept any non-empty path makes this test fail (reverted).
- Ancestor check: every existing folder on the way to a journal path is checked with `symlink_metadata`; a link is `Symlink { path }` (test asserts `systems/link`), and a non-folder is `Journal`. A journal that is a directory rather than a file is `Journal` ("it is not a file"); a linked journal is `Symlink`.
- Journal error texts never echo the parsed bytes: parse errors give only line and column; a bad path is shown with at most 256 input characters, escaped with `escape_debug`.
- The journal size is checked from metadata first and then the read is capped again (`read_capped` `TooLarge` mapped to `Journal`).
- Sweep: walks the root (only `*.aegis-new` files there) and `shared/`, `systems/` to the same depth as `collect`, skipping links and dot folders; deeper folders are not entered rather than refused. `.aegis-save.json.tmp` is removed only if it is a regular file.
- `save` after an I/O error part way through the commit leaves the journal; the folder's `written` map is not updated, so the next save stages the same files again and replaces the journal.

NEXT: reviewer
