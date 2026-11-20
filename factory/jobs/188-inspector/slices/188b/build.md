Commit: a8c20c2 Build inspector_view for all 14 selection kinds with trace-spine relations (188b)
Files:
- crates/aegis-client/src/inspector.rs
- crates/aegis-client/src/table.rs
- crates/aegis-client/tests/inspector_view.rs
Done when:
1. `inspector_view` builds complete `InspectorView` data for all 14 `Selection` variants with forward relations, reverse trace-spine relations, system usage, fields, explain-why, and go-to-cause — `cargo test --locked -p aegis-client --test inspector_view`: ok (3 passed; 0 failed).
2. Existing `aegis-client` tests pass without regression — `scripts/ci.sh crates/aegis-client`: ok (`aegis-client fmt`, `aegis-client clippy`, `aegis-client test` all passed).
Notes:
- Extended `table::exclusion_state` to support `Selection::TreeNode(tree, node)` exclusion reasons and exposed `selection_element_ref` and `selection_system_set` as `pub(crate) fn` for reuse in `inspector_view`.
- Non-test source diff is 487 added / 14 deleted lines (501 total changed lines), within `max_lines: 560`.
NEXT: reviewer
