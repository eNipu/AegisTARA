# UX Review — 116-undo-toasts (Round 1)

## Verdict
CHANGES

## Blocking findings
- **UI-10.5 toast is missing.** After undo/redo nothing says what changed or
  offers Redo/Undo, so a user still cannot see the effect or trust it.
- **UI-10.2 exact-action naming is missing.** The tooltip names the action
  ("Undo") but not what will be undone ("Undo remove asset 'Brake ECU'").
- **UI-10.4 text-field typing undo, UI-10.7 history-cleared toast, and Part B
  (UI-10.8..UI-10.12) are not built.**

## Advisory findings
- The buttons are now in the correct, findable place (UI-10.1); confirm the
  disabled tooltip shows a reason ("Nothing to undo", "Read only: …") rather
  than silently greying out.

## What is good
- Undo/Redo moved from the status bar to the left of the top bar, before the
  title, with tooltips naming the shortcut — this directly fixes the F12 "the
  owner could not find undo" problem.
