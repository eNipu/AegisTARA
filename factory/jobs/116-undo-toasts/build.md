# Build — 116-undo-toasts

## What was built (round 1)

### Undo/Redo first-class buttons (UI-10.1, part of UI-10.2/UI-10.3)
- `app.slint`: Undo and Redo are now icon-only buttons at the left end of the
  project top bar, before the project title, on every open-project page. The
  old status-bar text buttons are removed.
- Each button carries a hover tooltip "Undo (⌘Z)"/"Redo (⇧⌘Z)" from the one
  shortcut table (`undo-shortcut`/`redo-shortcut` string properties set by
  `shortcuts::install`), and an accessible name for screen readers.
- The Edit menu and the existing shortcut table already provided the ⌘Z /
  Ctrl+Z / ⇧⌘Z / Ctrl+Y / Ctrl+Shift+Z bindings and the menu accelerators; the
  buttons reuse the same `undo()`/`redo()` callbacks.

## Verified
- `cargo fmt --all -- --check`.
- `cargo clippy -p aegis-shell --all-targets -- -D warnings`.
- `cargo test -p aegis-shell` (ui, pages, monkey, small_windows, i18n,
  appearance, standalone, login) — all pass.

## Not built (remaining)
- **Tooltip/Edit-menu naming the exact action** ("Undo remove asset 'Brake
  ECU'") — the client does not yet expose a description of the pending
  undo/redo, so the tooltip names the action generically.
- **UI-10.4** text-field typing undo, **UI-10.5** the "Undone: … · Redo"
  toast + selection/scroll, **UI-10.6** the per-op undo test, **UI-10.7** the
  history-cleared toast.
- **Part B** (UI-10.8 to UI-10.12) removal-without-confirmation and the undo
  toast, inline field errors + sticky action-error toast, the confirmation
  allow list, and **UI-12.1/UI-12.2** (dialog rules + modal-overlay test).
