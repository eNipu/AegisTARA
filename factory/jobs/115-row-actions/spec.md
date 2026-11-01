# Spec — 115-row-actions

## Title
Row actions in tables (UI-09): a `DataTable` with per-row + and ⋯ actions, keyboard
navigation, read-only suppression and a 5,000-row performance check.

## Design Reference
- `docs/ui-ux-requirements.md` UI-09 (09.1 to 09.6), UI-08 (drafts to reuse), UI-10
  (undo + toast to reuse), UI-12 (no new modal dialog).
- `factory/backlog.md` 115-row-actions. Follows jobs 116 (undo/toasts) and 114
  (modeless creation).
- Affected paths: `crates/aegis-shell/ui/widgets.slint` (new `DataTable`),
  `crates/aegis-shell/ui/app.slint` (swap `Table` for `DataTable` on the 11 element
  tables and wire the new callbacks), `crates/aegis-shell/src/controller.rs`
  (row-action handlers and view-model guards), `crates/aegis-client/src/commands.rs`,
  `session.rs`, `view.rs` (draft context and duplicate edits),
  `crates/aegis-shell/tests/*`, `crates/aegis-client/tests/*`.

## Problem & Goal
Tables are plain read-only `StandardTableView` wrappers. There is no way to act on a
row without first selecting it and moving to the inspector, and no way to add a child
from a row. The user must know the page button and the inspector. UI-09 gives every
row two actions (+ add a child, ⋯ a menu) on hover, selection and keyboard focus, adds
real keyboard navigation, keeps the header sticky with truncation tooltips, and stays
fast at 5,000 rows.

## Build versus reuse

| Need | Decision | Why |
|---|---|---|
| Scrolling, content-y math, virtual-ish list layout | **Reuse** Slint `ListView` / `ScrollView` / `Flickable` (the same internals `StandardTableView` uses). | Bring-into-view and wheel/trackpad scrolling come free; no new dependency. |
| Column/row model types | **Reuse** Slint `TableColumn` and `StandardListViewItem` structs. | Keeps the existing `columns`/`rows`/`selected`/`picked` API so the 25 call sites change little. |
| Header + elided text | **Reuse** Slint `Text { overflow: elide }`; build the sticky header once. | `StandardTableView` cannot put a tooltip on an elided cell or force 28/32 px rows. |
| Per-row action buttons, tooltip, 28/32 px rows | **Build** a `DataTable` (one component in `widgets.slint`). | `StandardTableView` renders cells as plain `Text`; it cannot host buttons, custom row height, or per-cell tooltips. |
| Draft-in-inspector (job 114) | **Reuse** `DraftKind`, `BeginDraft`/`CommitDraft`/`CancelDraft`, `DraftForm`, `NameEditor`. Extend the draft context so + can pass a parent. | One creation pattern; the row + starts the same draft the page button starts. |
| Undo / toast of a removal (job 116) | **Reuse** the existing `Edit::Remove*` cascade edits and the undo toast. | Remove is one undo step and shows "… removed · Undo". |
| Duplicate | **Build** in the client core (`Duplicate*` edits), per type that can be cloned. | No library provides project-element cloning; one op per type, one undo step. |
| Shortcut spelling | **Reuse** `shortcuts.rs` as the only place a shortcut is spelled. | The `DataTable` key handler matches keys; it never writes `Ctrl+`/`⌘` text (the scan test stays green). |
| Modal dialogs | **None.** The ⋯ menu is an inline transient overlay inside the table, not a modal. | UI-12; `tests/dialogs.rs` must stay green. |
| New dependencies | **None.** | D5 allow list; smaller supply chain. |

## Design

- **`DataTable`** (`widgets.slint`) keeps the exact `Table` API — `columns: [TableColumn]`,
  `rows: [[StandardListViewItem]]`, `selected: int`, `label: string`, `picked(int)` —
  and adds row-action inputs: `editable`, `show-add`, `add-verb`, `show-open`,
  `show-duplicate`, `show-remove`, `compact`, and callbacks `activated(int)` (Enter:
  focus the inspector), `add-child(int)`, `open-row(int)`, `duplicate-row(int)`,
  `remove-row(int)`. It is built on `ListView`, so the scroll-into-view settle timer
  from the old `Table` is kept (say `set-current-row` again after 60 ms once the layout
  has a size).
  - Rows are 32 px (28 px when `compact`); the header is a fixed bar above the scroll;
    the name cell (column 1) elides and shows the full name in a `Tooltip`.
  - On hover, selection and keyboard focus each row shows a `+` button (when
    `show-add`) and a `⋯` button. The `+` accessible name is `add-verb + " " + name`
    ("Add asset to Brake ECU"). `⋯` toggles a small inline menu (Open, Duplicate,
    Remove) positioned under the row; menu buttons carry accessible names
    ("Open Brake ECU", "Duplicate Brake ECU", "Remove Brake ECU").
  - Keyboard (inside the `DataTable` focus scope): Up/Down/Home/End/PageUp/PageDown
    move the selection; Return fires `activated`; Control+Return fires `add-child`;
    Backspace or Delete fires `remove-row`; Control+D fires `duplicate-row`. `Control`
    is the platform command key on macOS (the shortcut table's convention).
- **Row actions by table** (UI-09.3): components → add asset bound to the component;
  assets → add a threat scenario on the asset; damage scenarios → add a threat scenario
  that leads to the damage; threat scenarios → add an attack tree linked to the threat;
  security controls → add a goal linked to the control; attack-tree rows keep the canvas
  controls (no +). Rows with no child (interfaces, data flows, trust boundaries, goals,
  claims, trees) show no `+`. Duplicate is offered where the element can be cloned
  (component, interface, data flow, trust boundary, asset, damage, threat, control,
  goal, claim); attack trees are not duplicated here (the library already copies trees).
- **Draft context** (client core): `UiState` gains `draft_damage`, `draft_threat`,
  `draft_control`; `draft_component` is reused for an asset bound to a component.
  `begin_draft` fills these from the current selection; the controller selects the
  parent row first, then sends `BeginDraft`. `commit_draft` produces one edit that
  creates the child and links it to the parent, so create is one undo step:
  `AddAssetBound`, `AddThreatLeading`, `NewTree { threat }`, `AddGoalFor`.
- **Duplicate** (client core): new `Duplicate*` edits clone the element (and its
  properties/links that the copy can own) with fresh ids and the pack's names.
- **Read-only** (UI-08.5 reused): the window sets `editable: root.editing`; a
  read-only user sees no row actions. The controller's `remove`/`duplicate` handlers
  also guard on `project.mode == Editing`.

## Slices (each test-first, then green and committed)

1. **S1 DataTable**: build `DataTable` with the same API as `Table` (columns, rows,
   selected, picked), 28/32 px rows, sticky header, elide + tooltip, keep the settle
   timer; swap it in for `Table` everywhere (non-element tables get no actions).
2. **S2 row actions**: + and ⋯ with accessible names; + starts a draft with the parent
   context (new client-core context fields and edits); ⋯ menu Open/Duplicate/Remove;
   Remove is immediate with the undo toast; Duplicate edits in the client core.
3. **S3 keyboard**: arrows move selection, Return focuses the inspector, Control+Return
   adds a child, Backspace/Delete removes, Control+D duplicates — driven with real key
   events (`common::press_key`, `press_chord`).
4. **S4 read-only + performance**: no row actions for a read-only user; a 5,000-row
   scroll/selection check on the large reference project with the measured number
   written down (not fabricated).

## Acceptance Criteria
1. **UI-09.1** `DataTable` replaces the `StandardTableView` wrapper with the same
   columns/rows/selected/picked API; built once for 084/085.
2. **UI-09.2** Two real buttons (+ and ⋯) on hover, selection and keyboard focus, with
   accessible names ("Add asset to Brake ECU"); ⋯ opens Open / Duplicate / Remove.
3. **UI-09.3** + adds, by row type: component → asset, asset → threat, damage → threat,
   threat → attack tree, control → goal; attack-tree nodes keep canvas controls; no
   child type shows no +.
4. **UI-09.4** Arrow keys move the selection; Return moves focus into the inspector;
   Control+Return adds a child; Backspace/Delete removes; Control+D duplicates where
   allowed.
5. **UI-09.5** 28 px compact / 32 px normal rows; sticky header; long names elide with a
   tooltip showing the full name.
6. **UI-09.6** Scrolling and selection stay fast at 5,000 rows; the measured number is
   recorded.
7. No new dependency; `cargo deny` unchanged; every new string uses `@tr` with ja and
   pseudo entries; `crates/aegis-shell/tests/dialogs.rs` stays green; no `.slint`/`.rs`
   file spells a shortcut (the `shortcuts.rs` scan stays green).
8. Tests are test-first: each slice's failing test is seen to fail for the right reason
   before the implementation.

## Out of scope
Multi-select (084), the density mode itself (085), library tree duplication, rich-text
descriptions, tree-node row actions (the canvas owns those). Hardware checks (IME,
VoiceOver/NVDA, a real GPU frame-rate run) are follow-ups recorded in the review and
decision, not code gates.

## Open questions (defaults taken)
1. Duplicate scope: clone element types that carry their own children/properties
   (component, interface, data flow, trust boundary, asset, damage, threat, control,
   goal, claim); attack trees are not duplicated here. Recorded as a decision.
2. "Open" means select the row and move focus into the inspector (the element's detail).
   A later job may special-case Open for threats (jump to the attack tree).
