GATE: PASS
Slice: 124d  Base: cd392127dda767d86a0ac73d89b2aa5632f3769f


Diff: 212 counted lines, 13 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test control_presets --test control_settings --test store
ok   cargo test --locked -p aegis-client --test undo_every_op --test undo_labels
