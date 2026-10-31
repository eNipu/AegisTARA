# UI Review — 116-undo-toasts (Round 2)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- The disabled and enabled Undo/Redo icons are close in tone in the top bar
  (`toast-removed.png`: both grey). Raise the enabled icon contrast, or colour it
  with `Theme.ink`.
- The tooltip text is white on Slint's default tooltip background; contrast in the
  dark appearance mode was not measured.
- The `FieldError` widget (icon + error colour) was not captured in a screenshot;
  the password mismatch and login error states are not in the screenshot set.
- The toast has no maximum height; a very long name is shortened to 40 chars in the
  client, so it wraps to at most three lines in practice.

## What is good
- The toast sits at the bottom centre of the work area, left of the inspector, and
  does not cover "Remove threat scenario" at 1360x860 (`toast-removed.png`).
- The toast uses the existing notice styling (severity colour border), an Undo or
  Redo text button and a dismiss X; one toast at a time for actions.
- The delete-project dialog has a clear red main button and a visible focus ring on
  Cancel (`toast-confirm-delete-project.png`).
