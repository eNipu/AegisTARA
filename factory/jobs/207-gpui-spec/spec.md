# 207 GPUI spec: screens, backend features, test scale

- Date: 2026-11-18
- Status: draft for the owner. Builds start after the owner signs off the mockup (job 206), except the explicitly authorized jobs recorded in the backlog. Architecture-review acceptance update: 2026-11-22; no additional mockup approval is implied.
- Mockup: https://claude.ai/artifact/XrQjbEY2w4tXkgiumEN89m (15 screens, clickable).
- Reads with: `docs/design/workbench-breakdown.md` (group 5), `docs/design/workbench-gui-first.md`, ADR 0066 (toolkit).

> **Update 2026-11-18 (owner review of the first mockup):** the navigation becomes a project explorer (one project; project definition, configurations, base library, shared analysis, systems with the same folders each). Each system has its own item definition, all text is Markdown, and systems inherit base attack trees through templates and integration properties. Outline and open questions: `docs/design/project-explorer.md`. New jobs 210 to 212 below; sections 1 and 3 are re-cut once the owner answers Q1 to Q6 there.

## TLDR

- The core already does most of the work. `aegis-client` holds the session, commands, workspace, view model, tables, pickers, find, reports and connection without depending on Slint. GPUI reuses that core, but the migration is not just a renderer swap: local GPUI owns a directly saved workspace, whereas Slint uses a Session and server even in standalone mode. Hosted GPUI editing remains job 225.
- **Backend work that is really missing** (section 3): safe session/GPUI transitions (218, 219), pinned calculation identities (223, 224), hosted editing in GPUI (225), control status (140), versions inside the app (203), settings layers with their source (171, 172), diagram positions for the item definition (194), and scoped API tokens (183). Workspace files, the command table, grids and inspector have landed; their failure-path gaps are follow-up jobs, not a reason to repeat those implementations.
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
| 13 | Project settings; layered overrides later | 209; layers in 190 after 171/172 | Existing settings/report parity does not require the new configuration model |
| 14 | Administration: users, roles, sign-in, tokens, license, audit (moves into App settings, 213) | 213 | Admin RPCs exist (049, 055); scoped tokens are 183 |
| 15 | Command palette | 182 | Find exists (081, `find.rs`) |

## 2. The shape of the GPUI app

- New crate `aegis-workbench` (binary `aegis`), depending on `aegis-client`, `aegis-canvas`, `gpui-pre` and `gpui-component` at the versions the spike pinned. One window: top bar, project tree, a tab area, inspector, bottom dock and status bar.
- Hosted target: an adapter sends `Command`s to the shared `Session` and turns its view-model updates into GPUI entity updates (225). The current hosted pane downloads a read-only snapshot; it must not acquire a separate mutable copy that bypasses the session. Local mode keeps its `Workspace`/`WorkspaceFolder` path and shares domain commands, undo and presentation logic, not an artificial network hop.
- Theme tokens from one file, light and dark; the colours in the mockup are the starting values.
- i18n through the existing Fluent catalogs (`i18n.rs`); the `.slint` `@tr()` strings move into the same catalogs.

### 2.1 Runtime and saved-state contract (review 2026-11-22)

| Mode | Current owner and persistence | Meaning of saved/confirmed | Remaining work |
|---|---|---|---|
| Slint hosted | Client Session owns optimistic Workspace; server actor commits to PostgreSQL | Accepted server sequence, not merely an optimistic edit | Commit reconciliation and drain/close (217, 218) |
| Slint standalone | Same Session over an embedded loopback server with FolderStore | Durable server append; stopping the server cannot save an unsent edit | Publication recovery and drain (215, 218) |
| GPUI local | Workbench owns Workspace and WorkspaceFolder; explicit Save | Folder save completed, not local acknowledgement or an uncommitted cell draft | Draft/save/leave contract and stale-navigation guard (219) |
| GPUI hosted | Session downloads project; workbench shows a read-only snapshot | Snapshot revision only; no hosted edit claim yet | Shared Session editing and confirmation (225) |

One transition policy must cover Save, close, quit, sign-out, reconnect and opening another project: validate/commit active editors, save or drain accepted work, resolve errors, then release resources and replace the workspace. A bounded failure must leave retry, cancellation or explicit discard available; no silent loss. Async completions carry origin/generation and cannot change a newer session or workspace. Keep UI logic for dialogs/focus in the toolkit adapter, but reuse command and persistence semantics in the client core. Do not create a second sync engine or force both local and hosted persistence through the same backend.

### 2.2 Migration and retirement acceptance

Job 207 records a capability matrix for local and hosted workflows, maintained through the screen jobs and finalized by 192. Each row names its implementation job, current state (built, read-only, missing or intentionally removed), acceptance test and evidence; planned checks are not marked passed. Include:

- Editing/undo, active drafts, save status, lease ownership/loss, reconnect/reload, pending submissions and failure/cancel behavior (214, 217 to 219, 225).
- Tree canvas/keyboard, item definition, problems/changes, versions/baselines, reports/exports, administration, accounts and licensing (187, 191, 195, 203, 209, 213); retain localization, platform and accessibility gates from section 4.
- Assessment semantics: pack content identity, engine compatibility and the exact revision used for a baseline or export (223, 224). A report must not claim reproducibility with an unavailable historical engine.
- Legacy FolderStore conversion (implementation job 227, after the version representation in 203): inventory project data, op/audit history, baselines, attachments and metadata; preserve them or explicitly archive unsupported parts with a user-visible explanation. Keep the source untouched, perform recovery on a staged copy if necessary, never silently treat the two representations as synchronized, and test interruption, reopening and rollback. Do not overwrite a changed destination on retry.
- One local and one hosted end-to-end TARA from creation/import through editing, save/reopen, baseline and report; include a failed save/disconnect recovery case. Screenshots are not evidence of persistence or workflow parity.

Job 192 cannot remove Slint until every shipped-workflow row passes or the owner explicitly accepts its removal. Existing completed jobs remain completed at their original scope; this contract tracks the remaining migration work. No new Slint features are added; narrow security/data-loss corrections keep it usable until retirement.

## 3. Backend features: what exists, what is missing

| Feature | Screens | State | Where or which job |
|---|---|---|---|
| Session, commands, undo and redo, optimistic edits | all | exists; transition/reconciliation gaps | `session.rs`, `commands.rs`, `workspace.rs`; 217, 218 |
| **Hosted editing in GPUI** | all editable screens | missing; current downloads are read-only | 225; shared Session, not another workspace authority |
| **Save/leave and asynchronous result isolation** | all | follow-up gaps | 214, 219 |
| **Pinned methodology and engine compatibility** | versions, reports, hosted editing | not fully enforced | 223, 224 |
| Connection, OIDC and password sign-in, transports | 1 | exists | `connection.rs`, `transport.rs`, jobs 040, 045, 060 |
| Standalone mode with license | 1 | exists for the old project file | job 107 |
| **Workspace folder open and save through the file set** | 1, 2 | built; draft/transition gaps remain | 164, 164e; follow-up 219 |
| Project list with lease holder | 2 | exists | job 058 |
| New project in three steps, from a template | 2 | missing | 204 (needs 200) |
| Process map, counts, lint marks | 3 | exists | job 131 |
| Lint and problems | 3 | exists | `Project::lint` |
| Item definition data, systems, components, interfaces | 4 | exists | jobs 074, 119 |
| **Item text sections and diagram positions** | 4 | missing | 193, 194 |
| Table columns and cells from one spec | 5 to 10 | exists | `view.rs`, `table.rs` |
| Link chips, relation registry, multi-link picker | 5 to 10 | exists | `relations.rs`, `picker.rs` |
| **Multi-cell edit, fill down, paste rows from Excel** | 5 to 10 | built | 189 |
| Column filters and find | 5 to 10 | built | 189 |
| Not-applicable rows with a reason | 5, 9 | model and grid/inspector support built; configuration workflow incomplete | 134, 188, 189; 135 planned |
| **Control status Implemented, Planned, Off** | 9 | missing | 140 |
| Explain-why and go to cause | 7, 8 | exists in the model | 188 wires it |
| Tree node operations, library, origin notes | 8 | exists | jobs 057, 065 |
| Shared steps, fragments, bypass trees | 8, 9 | missing; menu items stay disabled with a reason | BLK jobs 141 to 144 |
| **One command table: id, label, icon, tooltip, shortcut** | all | built | WBX-11, job 182 |
| **Versions inside the app: save, baseline, compare, restore** | 11 | missing (server history exists) | 203 |
| Report builder, XLSX, figures, system scope | 12 | exists | jobs 070 to 073, 121 |
| **Settings layers with "comes from" and reset** | 13 | missing | 171, 172 |
| Admin: users, roles, license, audit chain | 14 | exists | jobs 049, 055, 062 |
| **Scoped API tokens** | 14 | missing | 183 |

For each remaining screen, check the controller audit from 182 and move any still-needed non-toolkit behavior into `aegis-client` with tests before wiring the screen. Do not restart completed job 182 or copy its logic into a second UI-specific implementation.

## 4. Test scale

| Level | What | How many | Gate |
|---|---|---|---|
| Core | Unit and property tests in `aegis-model` and `aegis-client` | stays the bulk (about 345 client tests today); each new command above adds its own, including exact undo | `scripts/ci.sh` |
| Screen behaviour | GPUI headless tests in the test context: open the screen from a fixture, drive it with the keyboard only, check the view-model result | at least 3 per screen (open and read, one edit with undo, the main keyboard path); about 50 in all | each build job |
| Parity | For every test in `crates/aegis-shell/tests` (30 files), a named GPUI test or a written reason it no longer applies | one row per Slint test, kept in `factory/jobs/192-retire-slint/parity.md` | 192 cannot start until every row is green |
| Commands | Every command has a label, icon, tooltip and shortcut entry; every menu reads the one table | 1 table test | 182 |
| Screenshots | Each screen in light, dark, English, Japanese and pseudo-locale at 1440 x 900 and 900 x 600 | 15 screens x 5 variants | reviewed by eye per job; first check in 182 whether `gpui-pre` can render off screen, else capture the window |
| Performance | Cold start ≤ 1.0 s, frame ≤ 16 ms while panning `large` (100,000 nodes), interaction ≤ 50 ms, memory ≤ 300 MB | the spike's measure mode, kept as a bench | 182, 187, 189 |
| Failure/recovery | Deterministically delayed auth/download/submit results; save and commit failures; close with in-flight and queued edits | Each regression from 214 to 224 plus the local/hosted matrix in section 2 | 192; source inspection alone is not a pass |
| Migration | Legacy folder conversion, retained/archived history and attachments, interruption, retry and rollback | One fixture for each supported legacy representation, plus failure cases | 203/207 define policy; 227 implements/tests it; 192 requires evidence |
| Owner acceptance | One real TARA from start to report in GPUI, with local and hosted workflow evidence | Both modes; section 2.2 | 192 |

## 5. Risks and open points

1. **Screen readers.** ADR 0066 records that GPUI's screen reader support is weaker than Slint's. Job 051 built a screen reader path in Slint. Decide in 182: accept it for now (D37), or keep a minimum (focus order, accessible names) and measure it.
2. **Windows.** Only macOS was measured. The Windows and RDP measurement stays a backlog line and runs as soon as a Windows machine is there; it no longer blocks 182.
3. **Pinned pre-release GPUI.** `gpui-pre 0.3.12` and `gpui-component 0.9.1` are pinned. Upgrades are their own small jobs, never part of a screen job.
4. **Slint stays shippable until 192.** No new Slint features; narrow security/data-loss fixes are allowed. It keeps building and passing its tests until the owner has done one real TARA in GPUI and the retirement gates pass.

## 6. Job order

Execution order lives in `factory/backlog.md`; the 2026-11-22 safety queue precedes new features without resetting any in-flight job or granting mockup approval. Jobs 164/164e, 182, 208, 189 and 188 are already delivered at their recorded scope.

Dependencies for the remaining work:

- Safety and recovery: 214 to 222; 217 needs 215 and 216, and 218 needs 217. Other safety lines are independently scoped but run in backlog order.
- Reproducibility: 223 methodology pins → 224 engine/protocol contract.
- Approval: 206 mockups → 207 completed capability/migration contract before unlocking the remaining `[design]` screen jobs. The dependency audit is part of 207: a prerequisite must be delivered or queued before its dependant. In particular, resolve and queue or explicitly re-scope the 169 → 193 → 194 → 195 item-editor chain before unlocking 195; no implicit approval of that new model work.
- Hosted editing: 225 needs 214, 218, 219 and 224; it reuses the existing Session.
- Screen work: 187 canvas; 195 item editor after its approved prerequisite chain; 191's retirement slice uses existing client problems/change state after 182/219, with external-file changes (165) and task output (183/185) explicitly deferred to follow-ups. Jobs 203 versions and 209 reports/current project settings need 219/223/224; layered configuration settings (171/172/190) are separate, not a hidden dependency of 209. Record these scope splits in the owner-approved matrix. The Administration/Accounts/License portions of 213 must land before retirement; unrelated settings can follow.
- Legacy conversion: 227 implements the approved migration policy after 203, 215, 219, 223 and 224; it does not repeat the basic folder open/save implementation from 164.
- Retirement: 192 needs the safety/reproducibility work, 225, 227, those screen workflows and every acceptance gate in section 2.2. The current-state overview refresh is tracked separately as 226; changed runtime contracts are documented with their implementation, not left until 226.
