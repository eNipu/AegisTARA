Commit: none (stopped at 4a67828; worktree restored clean)
Files: none committed
Done when:
- Blocked by source line cap (`max_lines: 580`) before committing (L12, L14, L28).

Notes:
1. **Blocker — `max_lines: 580` cannot be met without violating L28**:
   A complete, compiling, `cargo fmt --all`-formatted implementation of every item in `189b.md` (with mandatory `missing_docs`, `# Errors`, and time-complexity doc comments) measured **1,611 changed source lines** excluding tests (`cargo check --locked -p aegis-client` passed with zero warnings):
   - `crates/aegis-client/src/commands.rs`: +37/-1 (`Edit::SetAssetProperties` + `to_ops` + `unchanged` + `bulk_label`)
   - `crates/aegis-client/src/lib.rs`: +1 (`pub mod grid;`)
   - `crates/aegis-client/src/grid.rs`: 1,573 lines broken down by section:
     - Constants, `GridError` (12 variants), `GridSelection` (4 fields + 8 methods), `CellEditorKind` (6 variants), `CellInput` (5 variants): **228 lines**
     - Helpers (`trunc64`, `push_edit`, `finish_batch`, `selection_name`, `target_matches_table`): **88 lines**
     - `cell_editor`, `edit_cell`, `edit_rating_cell`, `edit_cells`: **313 lines**
     - `sync_link_edits`, `read_cell_input`, `fill_down`: **158 lines**
     - `system_component`, `default_asset_for_owner`, `add_row_edit`, `delete_rows_edit`: **141 lines**
     - `format_tsv`, `parse_field`, `parse_tsv`: **138 lines**
     - Typed TSV parsers (`properties`, `impact`, `scales`, `treatment`, `links`), `paste_existing_row`, `paste_new_row` (all 7 editable tables), `paste_tsv`: **507 lines**

   Even after factoring out shared error/lookup helpers cleanly (without `#[rustfmt::skip]` or ugly compression per L28), the minimum idiomatic `rustfmt`-formatted size is ~1,100–1,200 source lines (~2x the 580-line cap).

   **Suggested re-cut (2 slices, or 3 slices if staying strictly under 580 each)**:
   - **Slice `189b1` (~550–650 lines)**: `Edit::SetAssetProperties` in `commands.rs`, `lib.rs` export, and core `grid.rs` (`GridError`, `GridSelection`, `CellEditorKind`, `CellInput`, `cell_editor`, `edit_cell`, `edit_cells`, `add_row_edit`, `delete_rows_edit`) + tests.
   - **Slice `189b2` (~550–650 lines)**: `fill_down` (including editable `Links`) + TSV clipboard (`MAX_PASTE_*`, `format_tsv`, `parse_tsv`, `paste_tsv` for existing & appended rows across all 7 tables) + tests.
   (Alternatively, split into 3 slices under 550 lines each: `189b1` core inline/multi-cell edit + selection + `SetAssetProperties`, `189b2` `fill_down` + `add_row_edit` + `delete_rows_edit` + `format_tsv`/`parse_tsv`, `189b3` `paste_tsv`.)

2. **Secondary prompt fix to include when re-cutting**:
   - `189b.md` lines 13, 73, 81, 109 refer to `Edit::Batch(edits).prune(project)`, whereas the existing method in `crates/aegis-client/src/commands.rs:3030` is `Edit::changing(self, project: &Project) -> Edit`.

3. **No other blockers**:
   - All `aegis-model` and `aegis-client` APIs needed by `189b` exist and `cargo check --locked -p aegis-client` compiled cleanly on the full implementation.
   - All 6 follow-up `Edit` variants (`SetAssetComponent`, `SetImpact`, `SetThreatScaling`, `SetTreatment`, `LinkGoalParty`, `LinkClaimParty`) do not read the element from `&Project` in `to_ops`, so `Edit::Batch` works for appending new rows with initial values in one undo step.

NEXT: planner
