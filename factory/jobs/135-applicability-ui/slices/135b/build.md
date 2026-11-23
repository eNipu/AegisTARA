Commit: afe8550 Format exclusion reasons in words and add condition switch toast (135b)
Files:
- crates/aegis-client/src/exclusion.rs
- crates/aegis-client/src/inspector.rs
- crates/aegis-client/src/lib.rs
- crates/aegis-client/src/session.rs
- crates/aegis-client/src/table.rs
- crates/aegis-client/tests/excluded_reasons.rs
- crates/aegis-client/tests/condition_toast.rs

Done when:
1. Banner and rows show the new text, equal for one selection — `cargo test --locked -p aegis-client --test excluded_reasons`: passed (1/1)
2. The switch toast has the exact format — `cargo test --locked -p aegis-client --test condition_toast`: passed (1/1)
3. Older table and inspector tests still pass — `cargo test --locked -p aegis-client --test table_features --test inspector_view`: passed (6/6, 3/3)

Notes:
- `cargo fmt --all` and `scripts/ci.sh crates/aegis-client` passed (fmt, clippy, and all tests).
- `cargo test --locked -p aegis-workbench --test table_view --test inspector_view` ran and passed without changes (10/10 and 4/4).
- In the ISO 21434 methodology pack, the first component's visual ID is `C00` (not `CP00`), so `excluded_reasons.rs` asserts the live visual ID returned by `pages::selection_visual_id` in `Owner {comp_vis} Gateway is excluded`.

NEXT: reviewer
