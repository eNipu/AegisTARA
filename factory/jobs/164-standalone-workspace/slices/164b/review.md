VERDICT: PASS
## For the owner
What changed: `WorkspaceFolder::save` now stages every changed file, writes a synced roll-forward journal (`.aegis-save.json`), then commits and removes the journal.
`open`, `create` and `convert_beside` run `recover` after taking the lock: they check the whole journal first, then finish it, then sweep stray `*.aegis-new` and `.aegis-save.json.tmp` files.
New tests in `tests/folder_recovery.rs`, ADR 0071, and one row in `docs/architecture.md`.
Look at: crates/aegis-client/src/folder.rs `read_journal` (~line 468): the trust boundary. It checks the size from metadata and caps the read again, uses `deny_unknown_fields`, limits the list lengths, checks each path with `valid_journal_path`, and checks every existing ancestor with `symlink_metadata`. All of this runs before `recover` acts on anything.
crates/aegis-client/src/folder.rs `open` (~line 184): the planned change of order. With no `aegis.json` and no journal, `open` returns `NotAWorkspace` before it takes the lock, so picking the wrong folder leaves no lock file behind. With a journal present, it takes the lock, recovers, then checks again.
crates/aegis-client/tests/folder_recovery.rs `a_hostile_journal_is_refused_and_touches_nothing`: each bad path comes after a valid entry, so code that acted on entries before checking all of them would change `shared/damage.json` and fail the test.
Risk: A hostile journal could escape the folder through `..`, absolute paths, backslashes or linked ancestors. Path validation and ancestor checks refuse these, and tests cover every such case, including through a symlink. A valid journal can still remove or replace any `*.json` inside the folder, including `.git/*.json`. That is as the prompt specified, and the damage stays inside the folder the user opened. If an I/O error stops a commit part way within a session, `written` stays stale. That gap already existed in 164a. The next save stages the same files again.
View: git -C factory/worktrees/164-standalone-workspace diff 76d3b8a68c718ebd3ccf50c332e04ee7646a83e0..HEAD
## Findings
none. Each "Done when" item is met, and each test would fail if its feature were removed:
- AfterStaging: without the sweep, `leftovers` would not be empty.
- AfterJournal: without roll-forward, the folder would open as the old project and the tree file would remain.
- Hostile journal: confirmed by the builder's mutation run.
- Link journal: without the ancestor check, the outside victim would be deleted.
- Oversized journal: refused with the "larger" message.
Not tested: the sweep removing `.aegis-save.json.tmp`. The code for it is trivial and only removes a regular file.
## Lesson
none
NEXT: human
