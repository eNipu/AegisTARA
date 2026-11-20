VERDICT: PASS
## For the owner
What changed:
Added spreadsheet `fill_down` (and `pub(crate)` `sync_link_edits`) in `crates/aegis-client/src/grid/fill.rs` to copy names, ratings (`properties`, `impact`, `scales`, `treatment`), and editable 1:1 / N:M link relations across selected rows in a single undoable batch without emitting empty no-op batches.
Added size-capped strict RFC-4180 TSV codec (`MAX_PASTE_BYTES`, `MAX_PASTE_ROWS`, `MAX_PASTE_COLS`, `format_tsv`, `parse_tsv`) in `crates/aegis-client/src/grid/tsv.rs` and re-exported both APIs from `aegis_client::grid`.
Extended `crates/aegis-client/tests/grid_edit.rs` with end-to-end fill-down + exact single-step undo tests, guard tests, TSV round-trip tests, and isolated limit/quoting error tests.
Look at:
- `crates/aegis-client/src/grid/fill.rs:26-76` — `sync_link_edits` minimal diff logic for 1:1 vs N:M relations (direct replacement without redundant unlink; order-preserving deterministic iteration; reused by `paste_tsv` in 189b4).
- `crates/aegis-client/src/grid/fill.rs:96-185` — `fill_down` bounds clamping, per-cell rename no-op check (line 144, because `Edit::Rename*` is not in `Edit::unchanged`), and empty-batch guard.
- `crates/aegis-client/src/grid/tsv.rs:87-184` — `parse_tsv` and `parse_quoted_field` enforcing byte/row/column caps during parsing and 1-based line tracking across multiline quoted fields.
Risk: Untrusted clipboard TSV could trigger OOM, panics, or misreported line numbers, and `fill_down` could emit empty undo batches or lose data on undo; checked that byte/row/col caps fail before allocating past the limit, `parse_tsv` has no panicking indexing/unwraps and terminates on all inputs, `fill_down` rejects no-op batches (including identical names), and tests verify exact `project.save()` byte restoration on single undo and isolated guards for every error variant.
View: git -C factory/worktrees/189-table-tabs diff efbc1a045fd290fac7bd3d5790f77d5634d0d594..HEAD
## Findings
none
## Lesson
none
NEXT: human
