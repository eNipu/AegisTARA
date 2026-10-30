# Spec — 081-command-palette

## Title
Command palette and the single shortcut table (UI-01, UI-07.6, UI-11.5)

## Design Reference
- Reference: docs/requirements.md UI-01; docs/ui-ux-requirements.md UI-07.6 and UI-11.5.
- Affected crates/paths: crates/aegis-shell/src/{shortcuts,controller,lib}.rs, crates/aegis-shell/ui/{app,widgets}.slint.

## Problem & Goal
The app has hand-written `Ctrl+...` shortcut text and no way to reach pages and commands from the keyboard. Add a Ctrl+K / ⌘K command palette over pages and commands with shortcuts shown, built from one shortcut table that also feeds the cheat sheet, and move the keyboard cheat sheet into the palette.

## Acceptance Criteria
1. Ctrl+K / ⌘K opens a transient palette (Escape closes, focus returns); 2. the palette searches pages and commands and shows their shortcuts; 3. one shortcut table is the single source for the palette and the cheat sheet, showing ⌘/⌥/⇧ on macOS and Ctrl/Alt/Shift elsewhere; 4. the keyboard cheat sheet is a view inside the palette and replaces the shortcuts dialog; 5. no new modal chrome beyond the transient palette popup.
