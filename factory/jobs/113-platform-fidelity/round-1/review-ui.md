# UI Review — 113-platform-fidelity (Round 1)

## Verdict
CHANGES

## Blocking findings
- **Appearance modes are missing (UI-07.5, acceptance criterion 6).** The
  splash/About/brand headers already stay obsidian, but there is no System
  default, no live OS light/dark switch, and no appearance setting, so the
  "native on macOS/Windows" goal is only partly met.
- **Monospace surfaces are not styled (UI-07.7).** `mono-font-family` is set
  but no identifier/JSON/detail surface uses it yet, so those panes still
  render in the proportional UI font.

## Advisory findings
- Menu accelerators now come from the table, so the mac menu shows ⌘ labels
  and the other platforms show Ctrl labels consistently. Verify the native
  macOS menu bar renders (Slint `MenuBar`) during the owner spike.
- Japanese and pseudo-locale screenshots should be re-taken after the font
  binding so a reviewer can confirm no tofu and no clipping.

## What is good
- `cupertino` on macOS and `fluent` elsewhere is chosen from the target OS in
  `build.rs`, and the existing brand tokens stay on top of the style, so
  layout and colours are unchanged.
- UI text now uses the native family through `default-font-family`, which is
  the foundation for the remaining font work.
