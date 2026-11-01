# Code Review — 114-modeless-create (Round 1)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- `DraftKind` and the per-kind `match` arms in `session.rs` grow with each element type; fine at eleven kinds.
- The controller's `push_draft_row` plus per-table patches repeat a pattern; a `DataTable` (job 115) can absorb it.
- `NameEditor` saves from `changed focused`, which Slint runs on the next turn; a rename could in theory reach the
  controller after a selection change. The test (row click while typing) shows the name goes to the right element.

## Standards Check
- The draft state, required fields and the one-op commit live in the client core; the window reports and shows.
- Every slice was test first; each red failed for a real reason, and two of them found real bugs (the threat page
  not refreshing on a draft change, an empty name equal to the "already sent" marker).
- Tests: client session tests for each kind (cancel and empty send no op, required fields, refusal, undo),
  window tests with real key and click events (create, cancel, create-and-next, required fields, item definition,
  name editor, shortcut, read-only), the five-assets count.
- fmt, clippy `-D warnings`, `cargo test --workspace --locked` (84 binaries, 0 failed), `scripts/deny.sh` pass.
- No new dependency; no shortcut spelled outside `shortcuts.rs` (that test still passes).
