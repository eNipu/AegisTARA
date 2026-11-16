Commit: 9572b70 Add, rename, remove and move systems as edits (119e)
Files: crates/aegis-client/src/commands.rs, crates/aegis-client/src/undo_label.rs, crates/aegis-client/tests/systems.rs
Done when:
1. cargo test --locked -p aegis-client --test systems: 7 passed
2. cargo test --locked -p aegis-client --test undo_labels --test pages: 22 and 4 passed
3. cargo check --locked -p aegis-shell: ok
Before commit: cargo fmt --all, scripts/ci.sh crates/aegis-client: all passed.
Notes: Non-test source diff is 185 lines (cap 300). MoveToSystem to a bulk label also added to bulk_label (move components). A repeated ID in MoveToSystem is moved once. RemoveSystem also clears the system's attributes before removal, per the prompt.
NEXT: reviewer
