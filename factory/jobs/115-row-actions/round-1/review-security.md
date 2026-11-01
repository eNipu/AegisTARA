# Security Review — 115-row-actions (Round 1)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- Remove and Duplicate callbacks are not guarded in the controller; the client core refuses edits outside Editing mode, and a test calls both callbacks as a read-only user and sees no change. A controller guard would be belt and braces.

## Security Evaluation
- No new network, file or credential surface and no new dependency (`scripts/deny.sh` passes). Every action sends the same typed edit as before, through the same lease and permission checks.
- The row keys (Delete, Control+D, Control+Return) do nothing when the table is not editable; the buttons are not drawn for a read-only user.
- Remove is one undo step with a toast (job 116), so a stray Delete is recoverable.
