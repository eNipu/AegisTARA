Commit: de2095a47c2c4811f2e4cf9c97c6f479b3f4b321 Add grid TSV paste engine and expose table selection helpers (189b4)
Files:
- crates/aegis-client/src/table.rs
- crates/aegis-client/src/grid/mod.rs
- crates/aegis-client/src/grid/edit.rs
- crates/aegis-client/src/grid/paste.rs
- crates/aegis-client/tests/grid_edit.rs

Done when:
1. All grid editing, fill down, TSV codec, and Excel paste tests pass with exact undo — `cargo test --locked -p aegis-client --test grid_edit` -> PASS (27 passed, 0 failed)
2. Client crate CI passes clean — `scripts/ci.sh crates/aegis-client` -> PASS (`fmt`, `clippy`, `test` all ok)

Notes:
- Made `aegis_client::table::selection_label` public (`#[must_use]` with doc comment) and added `#[must_use] pub fn selection_exclusion(p: &Project, sel: Selection) -> (bool, Option<String>)`.
- Made `grid::edit::with_row` `pub(crate)` for reuse by `grid::paste`.
- Implemented `paste_tsv` in `crates/aegis-client/src/grid/paste.rs` (re-exported from `aegis_client::grid`) covering all 7 editable tables (updating existing rows and appending new rows with `::new_v7()` IDs in one undoable `Edit`), dual vocabulary + methodology default label matching for properties and impacts (preserving `format_tsv` -> `paste_tsv` round-trips in all locales including `Language::Ja`), exact `TREATMENTS` ordering, 1:1 link multi-token rejection, case-insensitive link matching precedence (`visual_id` -> `"{visual_id} — {name}"` -> `name`), `default_threat_category` on appended threats/risks, `trunc64` error truncation, and no-op `Edit::Batch(vec![])` when pasting identical values onto existing rows.
- Source line count excluding `tests/`: 586 insertions, 2 deletions (588 total, under the 600-line cap).

NEXT: reviewer
