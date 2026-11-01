# Code Review — 115-row-actions (Round 1)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- `DataTable` is one 230-line widget; the key handler is a long `else if` chain. Fine until a second table kind needs its own keys.
- The twelve controller `on_duplicate_*` and `on_remove_row_*` handlers repeat one shape; a macro would shrink them, not worth it yet.
- `table-x/y/w` are window properties written by whichever table laid out last. They exist for tests; a page with two tables would report the wrong one.
- The duplicate edits carry a `suffix` string from the window (`@tr("copy")`), so the core stays free of i18n.
- The draft row is recognised by an empty ID cell in `DataTable`; a table whose first column can legitimately be empty would lose its actions.

## Standards Check
- Row semantics (draft context, duplicate edits, one undo step) live in the client core; the window only reports the row and shows the result.
- Test first in each slice; the S4 screenshot found a real layout defect that slice 1's tests could not (header drawn below the rows), fixed with a `VerticalLayout`.
- Tests: client session tests for each duplicate and draft context; window tests with real click and key events; read-only with a positive control; Return-to-inspector and draft-row tests (the latter seen to fail without its fix), a 5,000-row run with a changed-picture assertion.
- fmt, clippy `-D warnings`, `cargo test --workspace --locked` (1,632, 0 failed) and `scripts/deny.sh` pass on the final tree.
- No new dependency; the unused `Table` widget was removed.
