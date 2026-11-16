## Title
One table convention: `ColumnSpec`, `PageLayout` and per-page column specs (UI-14, client level)

## Design Reference
- `docs/architecture.md` III.9.1 (page table, `Page::layout()`, `ColumnSpec`)
- `docs/ui-ux-requirements.md` UI-14 and UI-14.1 to UI-14.3
- `docs/design/workbench-breakdown.md` WBX-07 (table tabs from `ColumnSpec`)
- `docs/decisions.md` D30 (one table convention), D37 (workbench), owner rule 2026-11-11 (no new Slint work)
- Code: `crates/aegis-client/src/view.rs` (`Page`, `PageGroup`), `crates/aegis-client/src/pages.rs`, `crates/aegis-client/tests/pages.rs`

## Problem & Goal
Every table page currently has its own ad hoc row shape and no shared declaration of its columns, so the workbench (job 189) cannot generate a tab from data and each new element type re-implements column code. This job publishes the convention in `aegis-client`: a `ColumnSpec` list per table page, with column kinds `Id`, `Name`, `Links`, `Computed`, `Rating`, `Text`, plus `Page::layout()` returning `Table(...)` for the table pages. Business value: one source of truth for every table's columns, so the GPUI shell renders all of them without per-page layout code, and computed columns are visibly non-editable by construction (rule P7).

## Acceptance Criteria
1. `ColumnKind` (Id, Name, Links(RelationKey), Computed, Rating, Text) exists in `aegis-client::view`, with `editable()` returning false for `Id` and `Computed`.
2. `ColumnSpec { kind, label: &'static str, width: u16, visible: bool }` exists.
3. `RelationKey` type alias exists (refined by job 149).
4. `PageLayout` enum exists with variants `Table(&'static [ColumnSpec])`, `Canvas`, `Custom`.
5. `Page::layout()` is implemented for every `Page` variant.
6. `ASSET_COLUMNS`, `DAMAGE_COLUMNS`, `THREAT_COLUMNS`, `CONTROL_COLUMNS`, `GOAL_COLUMNS` (and `RISK_COLUMNS`, `PROBLEM_COLUMNS`) are published in `aegis-client::view`.
7. Asset, damage, threat, control and goal pages declare `Table(...)` from their spec; each has Id then Name as its first two columns and at least one `Computed` column.
8. A test proves every table page's columns come from its spec and that computed columns refuse edits.
9. `cargo test -p aegis-client --test table_conventions` passes.

## Out of scope
- `ElementTablePage` in `app.slint`, link-chip click handling, the `Excluded` hatch in `theme.slint`, moving the Slint pages onto the convention, and light/dark screenshots. These are UI work and, under the owner rule of 2026-11-11 (no new Slint work), are left for the GPUI workbench job 189 (WBX-07), which reads these specs.
- The column chooser (job 129b) and the attack-tree table mode (job 152).

## Decisions
- 2026-11-16: scope reduced to the toolkit-independent client level only, following the precedent of job 119's rework: no `.slint` or `aegis-shell` changes, because the owner ruled no new Slint work and future UI is GPUI. The convention (`ColumnSpec`) is kept early because workbench jobs 188, 189 and 198 depend on it (workbench-breakdown section 3).
- `RelationKey` is a `&'static str` alias until job 149 builds the relation registry, so this job does not block on it.
- `ColumnSpec::width` is `u16` pixels, matching the existing Slint `TableColumn` width usage.