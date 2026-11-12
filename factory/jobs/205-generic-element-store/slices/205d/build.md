Commit: 1bee844 Link elements to threat scenarios and guard threat removal (205d)
Files: crates/aegis-model/src/{element.rs,op.rs,project.rs,error.rs}, crates/aegis-model/src/store/{op_log.rs,wire.rs}, crates/aegis-model/tests/elements.rs, crates/aegis-client/tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test elements` - 30 passed.
2. `cargo test --locked -p aegis-client --test undo_every_op` - 2 passed.
3. `scripts/ci.sh crates/aegis-model crates/aegis-client` - all steps passed.
4. `git diff --numstat HEAD~1 -- crates/aegis-model/src` - about 331 changed lines (cap 400).
Notes:
- `LinkNotAllowed` is unreachable through ops while the only kind allows threats; covered by a unit test in `element.rs` (`allows_reads_the_kind_table`) instead, as the prompt said.
- `inbound` is derived and updated in the link and unlink commits. `same_live` ignores it.
- Op log JSON: `{"op":"link_element","id":"...","target":{"threat":"<uuid>"}}`; `LinkWire` is in `wire.rs` for 205e to reuse.
- Tests build their own asset, damage and threat, because `record_every_op` does not cover element ops (walk_elements does).
NEXT: reviewer
