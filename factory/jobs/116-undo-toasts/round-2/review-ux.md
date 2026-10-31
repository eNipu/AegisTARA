# UX Review — 116-undo-toasts (Round 2)

VERDICT: CHANGES

## Blocking findings
- **UI-10.5 scroll into view is missing.** After Undo/Redo the page of the element
  opens and the element is selected (seen in `toast-undone.png`), but a long list is
  not scrolled to it. Acceptance criterion 5 asks for scrolling.
- **UI-10.4 is only partly proven.** Only the rename field (`NameEditor`) steps
  aside so the platform undo reaches its typing. No test sends the real ⌘Z through
  the native menu; whether a disabled menu item lets the key reach the field on
  macOS, Windows and Linux is unchecked.

## Advisory findings
- Removing and unlinking happen at once with a toast "Asset 'X' removed · Undo", and
  the toast says what else changed ("and unlinked from 3 threat scenarios", or "and
  1 other change" when the other changes are not unlinks). The generic phrase is
  vague for removals that cascade over trees or controls.
- Refusals ("That change is not possible: …") are now sticky error toasts. Login
  and membership errors and the password mismatch sit next to their field with an
  icon.
- The kept dialogs follow UI-12 (title names the thing, main button names the
  action, red, not the Enter default, Escape cancels, focus on Cancel); seen for
  delete project in `toast-confirm-delete-project.png`. Four kept confirmations are
  beyond the UI-12 list (delete user, remove member, password login off, log out),
  recorded with reasons in `dialogs.rs`; the owner may drop log out.
- Info toasts disappear after 8 s; a user who looks away loses the Undo button,
  though Ctrl+Z / ⌘Z and the top-bar button still work.

## What is good
- Undo/Redo are findable icon buttons whose tooltip names the action and shortcut,
  or says "Nothing to undo" / "Read only: …".
- Every Op kind is proven undoable, which is what makes the removed confirmations
  safe.
