# Security Review — 116-undo-toasts (Round 2)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- Removing confirmations means a mis-click removes data at once. The mitigation is
  Undo plus the per-op test; the actions undo cannot reverse (delete project,
  break lock, end sessions, delete user, remove member, password login off) still
  ask. Removing a member or user is not in the project history, and still asks.

## Security Evaluation
- No new network, file or credential surface. Undo/redo already went through the
  same server-checked edit path; the toast calls the same `undo()`/`redo()`.
- Element names appear in toast text and tooltips only, never in logs or errors
  sent to the server.
- The kept confirmations are unchanged in what they do; only their wording,
  focus and Escape behaviour changed. Enter no longer confirms a destructive
  action by default, which is safer than before.
- Dev-dependency `uuid` was already in the lockfile; `scripts/deny.sh` passes.
