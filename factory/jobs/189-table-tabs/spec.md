## Title
189-table-tabs — Element Grids, Inline & Bulk Editing, Column Filters, Link Chips, and Multi-Link Picker

## Design Reference
- `docs/design/workbench-breakdown.md` — Group 5: WBX-07, job `189-table-tabs`
- `docs/design/workbench-gui-first.md` — Sections 2, 3, 5a, 7 (spreadsheet grid editing, column filters, developer source view off by default)
- `docs/requirements.md` — `UI-14` (one table convention, computed cells, link chips, excluded-row hatch, column chooser), `UI-15` (multi-link picker), `WBX-07`, `WBX-11`
- `docs/decisions.md` — `D30` (relation registry & column kinds), `D37` (workbench shell), `D40` (GUI first), `D43` (business-logic edits, spreadsheet grids)
- `factory/jobs/207-gpui-spec/spec.md` — Sections 1–4 (core logic in `aegis-client`, thin GPUI views in `aegis-workbench`, headless keyboard tests)
- Existing code:
  - `crates/aegis-client/src/view.rs` (`ColumnKind`, `ColumnSpec`, `TableId`)
  - `crates/aegis-client/src/table.rs` (`Cell`, `TableRow`, `table_rows`)
  - `crates/aegis-client/src/relations.rs` (`RELATIONS`, `RelationKey`, `related`, `candidates`)
  - `crates/aegis-client/src/picker.rs` (`MultiLinkPicker`, `PickerFilter`, `PickerOption`)
  - `crates/aegis-client/src/command_table.rs` (`CommandSpec`, `COMMANDS`)
  - `crates/aegis-client/src/settings.rs` (`Settings`)
  - `crates/aegis-workbench/src/app/workbench.rs`, `panels.rs`, `commands.rs`, `icons.rs`, `files.rs`

## Problem & Goal
Analysts spend most of their TARA work editing lists of assets, damage scenarios, threats, the risk register, security controls, goals, and claims. Today the GPUI workbench (`aegis-workbench`) only renders explorer folders as read-only plain-text `ListTab` lines and shows an empty stub in the right-hand `InspectorPanel`.

**Business value (P7):** Turning every element folder into a live, spreadsheet-grade grid driven by `ColumnSpec` and the relation registry lets consultants perform daily TARA entry, bulk updates, Excel paste-in, filtering, and N:M linking directly inside the GPUI workbench—bringing the workbench to feature parity with spreadsheets for the core tabular work products without maintaining per-table UI code.

**Goal:**
1. Enrich `aegis-client` table data with structured link chips (from `relations.rs`), applicability exclusion state (`Project::applicability()`), explorer `Owner` scoping, per-column filters, remembered column visibility (`Settings`), and read-only developer workspace JSON extraction.
2. Add toolkit-independent grid editing commands in `aegis-client` (`grid.rs`): inline cell edits across every editable column kind, multi-cell edits, fill down, add/delete rows, and size-capped TSV copy/paste from Excel—all compiling to model `Edit`s with exact single-step undo.
3. Build the GPUI `MultiLinkPickerView` and relation-backed `InspectorPanel` (UI-15), and replace `ListTab` for all 8 `TableId` folders with a single `ColumnSpec`-driven `TableTab` (UI-14, WBX-07) supporting inline editing, multi-cell editing, fill down, clipboard copy/paste, column filters, column chooser, clickable link chips, excluded-row hatching, computed-cell styling, and a developer-only read-only source view.

## Acceptance Criteria
1. **Structured table data (`aegis-client`):** Every `Links` column populates `Cell::chips` from `relations::related`; every `TableRow` carries `excluded: bool` and `excluded_reason: Option<String>` from `Project::applicability()`; `table_rows_for_owner` filters all 8 `TableId`s consistently with `ExplorerIndex`; `table_source_json` returns the matching workspace JSON file for stored tables. Check: `cargo test --locked -p aegis-client --test table_features`.
2. **Column chooser & filters (`aegis-client`):** `Settings` remembers visible column keys per `TableId` (never hiding `ColumnKind::Id`) and defaults `developer_source_view` to `false`; `TableFilters` supports case-insensitive ASCII/CJK text queries and value-set checkboxes with AND semantics across columns. Check: `cargo test --locked -p aegis-client --test table_features`.
3. **Spreadsheet editing & Excel TSV paste (`aegis-client`):** `grid.rs` supports inline edit, multi-cell edit, fill down (including editable `Links`), add/delete rows, and TSV copy/paste (updating existing rows and appending new rows) for every editable column of every `TableId`; refuses non-editable columns, `Problems`, and oversized/malformed TSV inputs with specific `GridError` variants; every mutation is one `Edit` whose single undo restores saved snapshot bytes exactly. Check: `cargo test --locked -p aegis-client --test grid_edit`.
4. **Command table & i18n (`WBX-11`):** Every new grid command has a unique id, valid Lucide icon, translated label and tooltip in `en`, `ja`, and `pseudo`, a GPUI action handler, and a non-clashing key binding. Check: `cargo test --locked -p aegis-client --test command_table && cargo test --locked -p aegis-workbench --test command_wiring`.
5. **Multi-link picker & Inspector (`UI-15`):** Selecting an element populates `InspectorPanel` with its relation rows from `RELATIONS`, clickable link chips, and an inline keyboard-operable `MultiLinkPickerView` (type to filter, arrow keys, Space/Enter toggle keeping the picker open and refreshed, chip removal, Escape to close, single-step undo). Check: `cargo test --locked -p aegis-workbench --test picker_view`.
6. **Table tabs rendering (`WBX-07`, `UI-14`):** Opening any of the 8 `TableId` folders in `Workbench` opens a `TableTab` driven solely by `TableId::columns()` and `table_rows_for_owner()`, displaying computed marks/shading, clickable link chips (opening the target), risk tone pills, excluded-row hatch/badge, column chooser persisted to `Settings`, column filters, and a developer source view hidden by default until toggled. Check: `cargo test --locked -p aegis-workbench --test table_view`.
7. **Spreadsheet editing in GPUI (`D43`):** Headless keyboard-driven tests in `TableTab` perform inline name and rating edits, multi-cell edits, fill down, TSV copy/paste from clipboard (update + append), inline multi-link picker edits, add/delete rows, single-step undo restoring project bytes, and confirm read-only workspaces refuse every grid edit without changing the project. Check: `cargo test --locked -p aegis-workbench --test table_edit_view`.
8. **Workspace gate:** Full workspace passes formatting, clippy, and tests with zero regressions in existing Slint/client/model/workbench suites. Check: `scripts/ci.sh`.
9. **Owner visual check (L11):** Visual inspection in a live window (light & dark themes, English & Japanese) of computed column header mark/shading, link chips, excluded-row diagonal hatch, dropdowns, and multi-link picker layout.

## Out of scope
- Attack tree table mode with per-factor columns (job 152 / 187; `TableId::columns()` explicitly notes dynamic pack-factor columns are separate).
- Control status `Implemented` / `Planned` / `Off` column (job 140).
- Full inspector explain-why & go-to-cause (job 188; this job populates the inspector with the selection header and relation rows/multi-link picker required by UI-15).
- Editing the developer JSON source view (remains strictly read-only per D40/D43 and `workbench-gui-first.md` section 7).
- Changes to the workspace file format, snapshot format, or `aegis-shell` (Slint).

## Decisions
- **Re-export `Tone` from `aegis_client::table`:** avoids adding a new `aegis-canvas` dependency to `aegis-workbench` (`new_deps: false` across all slices).
- **Add `Edit::SetAssetProperties { asset, properties }` in `aegis-client`:** existing `Edit::SetAssetProperty` reads current properties from `&Project`, so batching two property toggles on the same asset in `Edit::Batch` would overwrite the first; a single `SetAssetProperties` client edit wraps the existing `Op::SetAssetProperties` cleanly for multi-property inline edits, fill down, and Excel paste.
- **TSV paste safety limits:** cap clipboard paste at `1 MiB`, `1,000` rows, and `64` columns (`MAX_PASTE_BYTES`, `MAX_PASTE_ROWS`, `MAX_PASTE_COLS`), rejecting oversized or unclosed-quote input before mutating state; reject unrecognized ratings/links with 1-based row/col error so no partial/corrupt paste lands silently.
- **Developer source view toggle:** stored in `Settings::developer_source_view` (default `false`) and toggled by command `"toggle-developer-source"` (palette only, no default shortcut); when off, no Source button or JSON ever appears (D40).
- **Non-table explorer folders:** folders without a `TableId` (`Components`, `Interfaces`, `DataFlows`, `TrustBoundaries`, `Trees`, `Parties`, `GoalsAndClaims`, `ProjectDefinition`, `SharedAnalysis`, `ItemDefinition`) keep `ListTab` until their dedicated jobs (187, 195) replace them.
