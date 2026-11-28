Commit: a7c4444 Wire Create from selection into GPUI control inspector (143d)
Files:
- crates/aegis-workbench/src/app/files.rs
- crates/aegis-workbench/src/app/inspector.rs
- crates/aegis-workbench/src/app/workbench.rs
- crates/aegis-workbench/tests/inspector_view.rs
Done when:
- GPUI control inspector "Bypass tree" + "Create from selection" headless test passes — `cargo test --locked -p aegis-workbench --test inspector_view`: 6 passed, 0 failed
- Client bypass tests pass against the complete workspace — `cargo test --locked -p aegis-client --test bypass_trees`: 4 passed, 0 failed
- Full workspace CI passes cleanly with zero warnings — `scripts/ci.sh`: ok workspace fmt, ok workspace clippy, ok workspace test (all steps passed in 9 crate(s))
Notes:
- Changed 80 source lines outside tests (1 in files.rs, 37 in inspector.rs, 42 in workbench.rs), within the 120-line cap.
- New test `control_inspector_creates_bypass_from_selection_with_read_only_and_undo_guards` verifies empty-selection refusal, `last_tree_selection()` retention across control selection, read-only refusal and status-bar note, fragment creation + control field/chip + step explain-why + intact source tree, undo restoration + fragment removal, and `last_tree_selection()` returning `None` after undoing or deleting the remembered tree.
NEXT: reviewer
