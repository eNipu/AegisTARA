# Decision — 116-undo-toasts

## Decision
ESCALATE

Round 2 built nearly all of UI-10 and UI-12: every `Op` kind is proven to undo and
redo exactly, undo/redo name the action and element, a bottom-centre toast
(Undo/Redo button, 8 s, live region, replaces the previous one) reports undo, redo,
removals and history-cleared, the 12 removal confirmations are gone, errors are
sticky toasts or inline field errors, the kept dialogs follow the UI-12 rules, and
the dialog rules and allow-list test are in. Code, security and UI reviews PASS.
fmt, clippy, `cargo test --workspace --locked` and `scripts/deny.sh` all pass.

It is not APPROVE because the UX review is CHANGES on two acceptance points:

1. **UI-10.5**: the affected element is selected on its page after Undo/Redo, but
   the list is not scrolled to it.
2. **UI-10.4**: only the rename field steps aside for the platform undo shortcut,
   and no test sends the real key through the native menu, so it is unproven on
   macOS, Windows and Linux.

Neither is an owner or hardware gate for item 1; item 2 needs a real window.

## Decisions to confirm
- Four confirmations beyond the UI-12 list are kept (delete user, remove member,
  password login off, log out), each with a reason in
  `crates/aegis-shell/tests/dialogs.rs`. Drop log out if you prefer.

## What to do next
`/factory rework 116-undo-toasts <note>`: scroll the list to the selected element
after undo/redo, and extend or prove the typing-undo stand-aside (a window test with
a real key, or a hardware check). Then the job can move to merged and backlog line
116 to `[x]`.
