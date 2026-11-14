Commit: 8d6cfdc A control remembers its preset with Op::SetControlKind (124d)
Files: crates/aegis-model/src/{control,op,project,error}.rs, methodology/mod.rs, store/{mod,wire,op_log,snapshot}.rs, tests/control_presets.rs (new); crates/aegis-client/src/{undo_label,commands}.rs, tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test control_presets`: 6 passed.
2. `cargo test --locked -p aegis-model --test control_presets --test store --test control_settings`: all passed.
3. `cargo test --locked -p aegis-client --test undo_every_op --test undo_labels`: all passed.
4. Changed lines outside tests: 199 added, 13 deleted (cap 380).
Before commit: `cargo fmt --all` and `scripts/ci.sh crates/aegis-model crates/aegis-client` passed.
Notes:
- `undo_labels.rs` lists no per-op labels, so it is unchanged. In `undo_every_op.rs` the second walk's count in the comments went from 28 to 29.
- No test asserts the size of `SecurityControl`, so nothing to report.
- `common::record_every_op` was not touched (out of scope); the tests that use it still pass.
- Removal of a preset control in the client pushes the `SetControlKind { kind: None }` op first, so it stays one undo step.
NEXT: reviewer
