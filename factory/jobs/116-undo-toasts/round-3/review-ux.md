# UX Review — 116-undo-toasts (Round 3)

VERDICT: PASS

## Blocking findings
None. Both round-2 points are closed:
- **UI-10.5**: after Undo/Redo the page opens, the element is selected and its row
  is scrolled into view, also in a 40-row list on a page that was not showing
  (`undo-scroll-long-list.png`). This also fixed a real bug: rows selected from
  outside were never scrolled to on any list.
- **UI-10.4**: a real ⌘Z/Ctrl+Z key event undoes the project when no field is dirty
  and leaves it alone while the rename field has unsaved typing (`typing_undo.rs`).

## Advisory findings
- The undo toast can cover the last visible row when the selected row is at the
  bottom edge (T39 in the screenshot). Leave a margin below the scrolled row, or move
  the toast above the list's bottom edge.
- Not checked by hand: the native macOS menu bar with a dirty field (headless test
  covers Slint's window-level shortcut path only), and VoiceOver/NVDA on the toast.
  Both are tracked as checks for a person, not gates for this job.
- Only the rename field has a saved value to differ from; other fields are for new
  items.
- Log out keeps its confirmation by owner decision.

## What is good
- Undo/Redo are findable and name the action; removals happen at once with an Undo
  toast; Undo says what it undid and offers Redo; errors are sticky toasts or inline
  with an icon; the dialogs that stay follow UI-12.
