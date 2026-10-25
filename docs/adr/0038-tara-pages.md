# ADR 0038: TARA pages in the desktop app

- Status: accepted (job 041-tara-pages)
- Date: 2026-10-25
- Scope: `crates/aegis-client/src/{pages,view,commands,outline,state}.rs`, `crates/aegis-shell/ui/`, `crates/aegis-shell/src/{controller,launcher}.rs`
- Rules: A1, D5, D7, P4 in `docs/engineering-standards.md`; decision D7
- Builds on: `0030-client-core.md`, `0031-desktop-shell.md`

## Context

The shell had one canvas view. A TARA needs every step of ISO/SAE 21434
clause 15 on screen: assets, damage scenarios, threats, attack paths,
feasibility, risk, treatment, goals and claims.

## Decisions

### 1. Pages follow the TARA steps

A sidebar lists the steps in order with counts: overview, assets, damage,
threats, attack trees, risks and treatment, controls, goals and claims,
problems, then versions, history, reports and admin. Menus repeat every
action with a shortcut, so the app is keyboard usable.

### 2. Page data is pure and lives in `aegis-client`

`pages.rs` turns a `Project` into rows and details. It has no UI code and is
unit tested. Rows are sorted by stable keys (D7, P4). The shell only copies
them into Slint models.

### 3. Edits are intents with safe cascades

The UI sends `Edit`s, not raw ops. An `Edit` expands to model ops. Removing
something that others use first removes the links, so the server never sees
an op that fails validation, and undo restores everything in one step.

### 4. Attack trees are edited as an outline

A text outline (indent = child, `OR`/`AND`/`SEQ` gates, same text = shared
step) covers every tree the model can hold and is fast with a keyboard. The
canvas shows the result. A drag and drop editor can come later.

### 5. Icons from Lucide

The menu and sidebar glyphs are Lucide SVGs (ISC license, allowed by D5),
copied into `ui/icons/` with the license file. No icon font, no network.

## Consequences

- The window has many properties. They are grouped by page in `app.slint`.
- Headless tests render every page and save screenshots
  (`target/shell-screenshots/`), so UI review can look at real pixels.
