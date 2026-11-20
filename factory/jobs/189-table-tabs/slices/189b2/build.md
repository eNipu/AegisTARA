Commit: efbc1a0 Add grid cell editor descriptors and single/multi-cell editing (189b2)
Files: crates/aegis-client/src/grid/edit.rs, crates/aegis-client/src/grid/mod.rs, crates/aegis-client/tests/grid_edit.rs
Done when:
- Inline edit, cell editor descriptor, read-only/stale/bounds guard, and multi-cell edit tests pass with exact undo — `cargo test --locked -p aegis-client --test grid_edit` (PASS: 11 passed, 0 failed)
- Client crate CI passes clean — `scripts/ci.sh crates/aegis-client` (PASS: fmt, clippy, test all passed in 1 crate)
Notes:
- Changed source lines excluding `tests/`: 544 (`crates/aegis-client/src/grid/edit.rs`: 540, `crates/aegis-client/src/grid/mod.rs`: 4), under the 550-line cap with standard `cargo fmt --all` and no `#[rustfmt::skip]`.
- `finish_batch` flattens nested `Edit::Batch` values (e.g. from `CellInput::Impacts` inside `edit_cells`) before calling `Edit::Batch(flat).changing(project)` so no-op inner batches are stripped cleanly and `bulk_label()` inspects leaf edits directly.
- `edit_cell` for `(Threats, "scales")` reads and preserves each threat's existing `scaling_reason`; both single-cell and 3-row multi-cell tests assert preservation and exact byte-for-byte single-step `ws.undo()`.
NEXT: reviewer
