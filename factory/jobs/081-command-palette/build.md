# Build — 081-command-palette

## What was built
- `crates/aegis-shell/src/shortcuts.rs`: the single shortcut table (id, label,
  group, macOS keys, other keys) plus `keys()`, `page_for()`, and
  `palette_entries()`. `⌘`/`⌥`/`⇧` on macOS, `Ctrl`/`Alt`/`Shift` elsewhere.
- The command palette: a transient popup opened with ⌘K / Ctrl+K (View menu
  accelerator), listing pages and commands with their shortcuts, filtered by
  the search field. Escape closes it and Slint returns focus to where it was.
- The keyboard cheat sheet is a view inside the palette (the **Keyboard
  shortcuts** toggle) and replaces the old shortcuts dialog; the Help menu
  opens the palette in that view.
- The controller dispatches palette picks to `ShowPage` for pages and to the
  matching `Command` for commands.

## Verified
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.
- `cargo test -p aegis-shell --test i18n`.
- `cargo test --workspace` (ran unsandboxed; loopback bind and OS trust store are blocked by the sandbox).
- `./scripts/deny.sh`.

## Not built (remaining)
- Full element search inside the palette (page navigation covers the elements
  on each page); job 113 reuses this shortcut table for the menus.
