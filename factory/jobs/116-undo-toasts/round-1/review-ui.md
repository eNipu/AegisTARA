# UI Review — 116-undo-toasts (Round 1)

## Verdict
CHANGES

## Blocking findings
- **No undo toast** (UI-10.9) and **no field-error / action-error treatment**
  (UI-10.11), so removals still need confirmation dialogs and errors are not
  surfaced inline.

## Advisory findings
- The tooltip uses white text on Slint's tooltip background; verify contrast in
  both appearance modes (or style the tooltip content from the palette).
- Keep the buttons enabled state visually obvious (disabled icon vs active).

## What is good
- The Undo/Redo buttons are icon-only, at the left of the top bar, before the
  title, and consistent across pages; the text buttons are gone.
- The accessible names and shortcut tooltips are wired from the one shortcut
  table.
