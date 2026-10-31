# Decision — 116-undo-toasts

## Decision
APPROVE

The whole UI-10 and UI-12 scope is built. Round 3 closed the last two points:
a real-key test shows the undo shortcut undoes the project normally and leaves it
alone while a saved field has unsaved typing (UI-10.4), and undo now scrolls the
restored row into view on every list (UI-10.5), after the test found that tables
made on a page switch never scrolled.

All four round-3 reviews PASS. fmt, clippy `-D warnings`,
`cargo test --workspace --locked` (82 binaries, 0 failed) and `scripts/deny.sh`
pass. The confirmation list stays as decided: the five UI-12 cases plus delete
user, remove member, password login off and log out (owner confirmed log out
stays).

## Follow-ups for a person (not gates)
- Try ⌘Z in a dirty rename field on a real macOS window (native menu bar).
- Check the toast live region with VoiceOver and NVDA.
- Advisory: the undo toast can cover the last scrolled row.
