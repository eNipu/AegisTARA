# 207 GPUI spec: screens, backend features, test scale

- Date: 2026-11-18
- Status: draft for the owner. Builds start after the owner signs off the mockup (job 206).
- Mockup: https://claude.ai/artifact/XrQjbEY2w4tXkgiumEN89m (15 screens, clickable).
- Reads with: `docs/design/workbench-breakdown.md` (group 5), `docs/design/workbench-gui-first.md`, ADR 0066 (toolkit).

> **Update 2026-11-18 (owner review of the first mockup):** the navigation becomes a project explorer (one project; project definition, configurations, base library, shared analysis, systems with the same folders each). Each system has its own item definition, all text is Markdown, and systems inherit base attack trees through templates and integration properties. Outline and open questions: `docs/design/project-explorer.md`. New jobs 210 to 212 below; sections 1 and 3 are re-cut once the owner answers Q1 to Q6 there.

## TLDR

- The core already does most of the work. `aegis-client` (26k lines, about 345 tests) holds the session, the commands, the view model, the pages, tables, pickers, find, reports and the connection. None of it depends on Slint. GPUI becomes a new renderer over the same core.
- **Backend work that is really missing** (section 3): workspace folder open and save (164), one command table with icons, tooltips and shortcuts (WBX-11), grid editing commands (multi-cell, fill down, paste rows), control status (140), versions inside the app (203), settings layers with their source (171, 172), diagram positions for the item definition (194), and scoped API tokens (183).
- **Rule:** a GPUI view holds no logic. Every number, label, cell and enabled state comes from `aegis-client`. Anything still computed in `aegis-shell/src/controller.rs` (4,819 lines) moves into `aegis-client` first, with its tests, before the GPUI screen that needs it is built.
- **Test scale** (section 4): core tests stay the bulk. Each screen gets keyboard-only headless tests in GPUI's test context, a parity list against the Slint tests it replaces, screenshots in light, dark, English, Japanese and pseudo-locale, and the four performance budgets on the `large` project.
- Spec-driven development is what the factory already does (backlog line → `spec.md` → plan → slices → gate → review), so no outside spec tool is added.

## 1. Screens

| # | Screen | Job | Notes |
|---|---|---|---|
| 1 | Sign in: company, password, on this computer | 208 | The five connection states of the Slint shell; the server picker; standalone folder and license |
| 2 | Projects: list, lease holder, new project in three steps | 208, 204 | Methodology chosen once (D42) |
| 3 | Overview: process map, risks by level, top risks, problems dock | 182, 191 | Process map data exists (131) |
| 4 | Item definition: text sections, diagram, systems | 195, 194 | Systems data exists (119) |
| 5 to 7, 9, 10 | Grids: assets, damage, threats and risks, controls, goals, claims, parties | 189 | `ColumnSpec` and `table_rows` exist (129, 129c) |
| 8 | Attack tree canvas with the node menu | 187 | `aegis-canvas` display list exists |
| — | Inspector with explain-why and go to cause | 188 | Explain-why exists in the model |
| 11 | Versions: timeline, compare, baseline, restore | 203 | `Project::diff` exists; the store inside the app does not |
| 12 | Reports: profile, scope, sections, language, checks, preview | 209 | Report builder exists (070 to 073, 121) |
| 13 | Settings with layers | 209 (needs 171, 172) | |
| 14 | Administration: users, roles, sign-in, tokens, license, audit (moves into App settings, 213) | 213 | Admin RPCs exist (049, 055); scoped tokens are 183 |
| 15 | Command palette | 182 | Find exists (081, `find.rs`) |

## 2. The shape of the GPUI app

- New crate `aegis-workbench` (binary `aegis`), depending on `aegis-client`, `aegis-canvas`, `gpui-pre` and `gpui-component` at the versions the spike pinned. One window: top bar, project tree, a tab area, inspector, bottom dock and status bar.
- One adapter between the core and GPUI: it sends `Command`s to the `Session` and turns view-model updates into GPUI entity updates. Views only read view-model data.
- Theme tokens from one file, light and dark; the colours in the mockup are the starting values.
- i18n through the existing Fluent catalogs (`i18n.rs`); the `.slint` `@tr()` strings move into the same catalogs.

## 3. Backend features: what exists, what is missing

| Feature | Screens | State | Where or which job |
|---|---|---|---|
| Session, commands, undo and redo, optimistic edits | all | exists | `session.rs`, `commands.rs`, `workspace.rs` |
| Connection, OIDC and password sign-in, transports | 1 | exists | `connection.rs`, `transport.rs`, jobs 040, 045, 060 |
| Standalone mode with license | 1 | exists for the old project file | job 107 |
| **Workspace folder open and save through the file set** | 1, 2 | missing | 164 (model side done in 162) |
| Project list with lease holder | 2 | exists | job 058 |
| New project in three steps, from a template | 2 | missing | 204 (needs 200) |
| Process map, counts, lint marks | 3 | exists | job 131 |
| Lint and problems | 3 | exists | `Project::lint` |
| Item definition data, systems, components, interfaces | 4 | exists | jobs 074, 119 |
| **Item text sections and diagram positions** | 4 | missing | 193, 194 |
| Table columns and cells from one spec | 5 to 10 | exists | `view.rs`, `table.rs` |
| Link chips, relation registry, multi-link picker | 5 to 10 | exists | `relations.rs`, `picker.rs` |
| **Multi-cell edit, fill down, paste rows from Excel** | 5 to 10 | missing as commands | 189; reuse the parsing in `excel.rs` |
| Column filters and find | 5 to 10 | exists for find; filters per column missing | 189 |
| Not-applicable rows with a reason | 5, 9 | in progress | 134 (in flight), 135 |
| **Control status Implemented, Planned, Off** | 9 | missing | 140 |
| Explain-why and go to cause | 7, 8 | exists in the model | 188 wires it |
| Tree node operations, library, origin notes | 8 | exists | jobs 057, 065 |
| Shared steps, fragments, bypass trees | 8, 9 | missing; menu items stay disabled with a reason | BLK jobs 141 to 144 |
| **One command table: id, label, icon, tooltip, shortcut** | all | missing | WBX-11, job 182 |
| **Versions inside the app: save, baseline, compare, restore** | 11 | missing (server history exists) | 203 |
| Report builder, XLSX, figures, system scope | 12 | exists | jobs 070 to 073, 121 |
| **Settings layers with "comes from" and reset** | 13 | missing | 171, 172 |
| Admin: users, roles, license, audit chain | 14 | exists | jobs 049, 055, 062 |
| **Scoped API tokens** | 14 | missing | 183 |

Anything the Slint controller computes that is not in this table is found in job 182's first slice: list every function in `controller.rs` and `adapter.rs` that is not toolkit glue, and move it into `aegis-client` with its tests.

## 4. Test scale

| Level | What | How many | Gate |
|---|---|---|---|
| Core | Unit and property tests in `aegis-model` and `aegis-client` | stays the bulk (about 345 client tests today); each new command above adds its own, including exact undo | `scripts/ci.sh` |
| Screen behaviour | GPUI headless tests in the test context: open the screen from a fixture, drive it with the keyboard only, check the view-model result | at least 3 per screen (open and read, one edit with undo, the main keyboard path); about 50 in all | each build job |
| Parity | For every test in `crates/aegis-shell/tests` (30 files), a named GPUI test or a written reason it no longer applies | one row per Slint test, kept in `factory/jobs/192-retire-slint/parity.md` | 192 cannot start until every row is green |
| Commands | Every command has a label, icon, tooltip and shortcut entry; every menu reads the one table | 1 table test | 182 |
| Screenshots | Each screen in light, dark, English, Japanese and pseudo-locale at 1440 x 900 and 900 x 600 | 15 screens x 5 variants | reviewed by eye per job; first check in 182 whether `gpui-pre` can render off screen, else capture the window |
| Performance | Cold start ≤ 1.0 s, frame ≤ 16 ms while panning `large` (100,000 nodes), interaction ≤ 50 ms, memory ≤ 300 MB | the spike's measure mode, kept as a bench | 182, 187, 189 |
| Owner acceptance | One real TARA from start to report in the GPUI app | 1 | 192 |

## 5. Risks and open points

1. **Screen readers.** ADR 0066 records that GPUI's screen reader support is weaker than Slint's. Job 051 built a screen reader path in Slint. Decide in 182: accept it for now (D37), or keep a minimum (focus order, accessible names) and measure it.
2. **Windows.** Only macOS was measured. The Windows and RDP measurement stays a backlog line and runs as soon as a Windows machine is there; it no longer blocks 182.
3. **Pinned pre-release GPUI.** `gpui-pre 0.3.12` and `gpui-component 0.9.1` are pinned. Upgrades are their own small jobs, never part of a screen job.
4. **Slint stays shippable until 192.** No new Slint work, but the Slint app keeps building and passing its tests until the owner has done one real TARA in GPUI.

## 6. Job order

206 mockups (owner sign-off) → 207 this spec → 164 workspace folder → 182 skeleton (command table, tree, palette, themes, controller audit) → 208 sign-in and projects → 189 grids (with 140 control status) → 188 inspector → 187 tree canvas → 195 item editor (after 193, 194) → 191 bottom dock → 203 versions → 209 reports and project settings, 213 app settings later → 192 retire Slint.
