Commit: 4622528 Set and clear element attributes (205c)
Files: crates/aegis-model/src/{element.rs,op.rs,project.rs,error.rs}, crates/aegis-model/src/store/{op_log.rs,wire.rs}, crates/aegis-model/tests/elements.rs, crates/aegis-client/tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test elements` - 24 passed.
2. `cargo test --locked -p aegis-client --test undo_every_op` - 2 passed.
3. `scripts/ci.sh crates/aegis-model crates/aegis-client` - all steps passed.
4. `git diff --numstat HEAD~1 -- crates/aegis-model/src` - 241 added, 10 removed (about 251), under 400.
Notes:
- `ModelError::EmptyActor` is for a blank non-empty actor such as "  "; an empty actor is allowed. The provenance-refusal test uses "  ".
- A `RemoveElement` guard (`ElementHasAttributes`) runs after `UnknownElement`. Its inverse is still a plain `AddElement`.
- `AttrView` is in op_log.rs and `AttrWire` in wire.rs. `AttrWire` is ready for 205e.
- Removed the `dead_code` allows in element.rs that were waiting for this slice.
NEXT: reviewer
