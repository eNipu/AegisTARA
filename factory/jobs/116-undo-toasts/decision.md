# Decision — 116-undo-toasts

## Decision
ESCALATE

Round 1 delivered the first, highest-value slice: Undo/Redo are now visible
icon buttons at the left of the project top bar on every page, with shortcut
tooltips, and the hidden status-bar text buttons are gone. Code and security
reviews PASS and all shell tests pass.

The job is not approvable yet. The UX/UI reviews are CHANGES because the
remaining scope is still to build:

- The "Undone: … · Redo" toast and per-action undo/redo naming (UI-10.2/10.5),
  text-field typing undo (UI-10.4), the history-cleared toast (UI-10.7), and
  the per-op undo test (UI-10.6).
- Part B: undoable changes happen at once with an undo toast (UI-10.8/10.9/10.10),
  inline field errors and a sticky action-error toast (UI-10.11), and the
  confirmation allow list (UI-10.12), plus the UI-12 dialog rules and
  modal-overlay test.

## What to do next
`/factory rework 116-undo-toasts <note>` to continue with the toast, the undo
description, the per-op test, and Part B. Code and security are green for this
slice.
