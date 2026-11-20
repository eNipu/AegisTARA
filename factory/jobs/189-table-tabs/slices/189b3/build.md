Commit: e25e2e6 Add grid fill down and strict RFC-4180 TSV codec (189b3)
Files:
- crates/aegis-client/src/grid/fill.rs
- crates/aegis-client/src/grid/tsv.rs
- crates/aegis-client/src/grid/mod.rs
- crates/aegis-client/src/grid/edit.rs
- crates/aegis-client/tests/grid_edit.rs
Done when:
1. `fill_down` (including editable links and no-op guard) and strict RFC-4180 TSV codec tests pass with exact undo — `cargo test --locked -p aegis-client --test grid_edit` -> PASS (17 passed, 0 failed)
2. Client crate CI passes clean — `scripts/ci.sh crates/aegis-client` -> PASS (`ok aegis-client fmt`, `ok aegis-client clippy`, `ok aegis-client test`)
Notes:
- Counted source diff excluding `tests/`: 450 changed lines (449 added, 1 deleted) across `crates/aegis-client/src/grid/{edit,fill,mod,tsv}.rs`, within the 520-line cap.
- `Edit::Rename*` is not listed in `Edit::unchanged` in `commands.rs`, so `fill_down` checks `element_name(project, dst) == Some(src_name.trim())` before emitting a rename edit so filling identical rows never produces a no-op rename and returns `Err(GridError::NothingToFill)`.
- `sync_link_edits` is `pub(crate)` in `grid/fill.rs` ready for reuse by `paste_tsv` in 189b4.
NEXT: reviewer
