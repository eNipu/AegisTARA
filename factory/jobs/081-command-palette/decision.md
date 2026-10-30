# Decision — 081-command-palette

## Decision
One shortcut table drives the palette and the cheat sheet, and the cheat sheet
is a view inside the palette. The palette is a transient popup, not a modal
dialog in the UI-12 sense (it is closed by Escape and by picking, and holds no
work context).

## Key choices
- `shortcuts.rs` is the single source: each entry has an id, a label, a group,
  and platform key text (`⌘`/`⇧` on macOS, `Ctrl`/`Shift` elsewhere). Job 113
  will read the same table for the menus (UI-07.6).
- The palette lists every `Page` plus every command with a shortcut; picking a
  page navigates, picking a command runs its `Command`.
- The shortcut list moves into the palette as the **Keyboard shortcuts** view
  (UI-11.5), and the old shortcuts dialog is removed.
