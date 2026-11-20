Commit: 4210609 Wire InspectorPanel explain-why, go-to-cause, and TreeNode navigation (188c)
Files:
- crates/aegis-workbench/src/app/commands.rs
- crates/aegis-workbench/src/app/inspector.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/tests/inspector_view.rs

Done when:
1. `"go-to-cause"` command is wired to an icon, action handler, and key binding — `cargo test --locked -p aegis-workbench --test command_wiring` -> 4 passed; 0 failed
2. All existing `picker_view` and `InspectorPanel` tests pass — `cargo test --locked -p aegis-workbench --test picker_view` -> 9 passed; 0 failed
3. Headless GPUI tests for inspector trace-spine neighbours, explain-why, and go-to-cause (risk -> deciding step -> capping control, and excluded element -> off condition) pass — `cargo test --locked -p aegis-workbench --test inspector_view` -> 4 passed; 0 failed
4. Full workspace CI passes — `scripts/ci.sh` -> ok workspace fmt, ok workspace clippy, ok workspace test (all steps passed in 9 crates)

Notes:
- Counted source diff lines (`git diff --numstat -- src crates/*/src`): 325 added lines against `max_lines: 560`.
- `Workbench::open_target_skipping` reveals and opens the owning `Selection::Tree(tree_id)` explorer row when navigating to `Selection::TreeNode(tree_id, node_id)` while forwarding the full `Selection::TreeNode(tree_id, node_id)` to `InspectorPanel`.
- `InspectorPanel::open_chip` passes `Some(cx.entity_id())` to `Workbench::open_target_skipping` and computes `inspector_view` in a separate borrow scope before `apply_built` calls `cx.notify()`, preventing GPUI double-leases.

NEXT: reviewer
