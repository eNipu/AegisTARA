GATE: PASS
Slice: 133a  Base: b8d70b9


Diff: 602 counted lines, 12 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --lib id
ok   cargo test --locked -p aegis-model --test conditions
ok   cargo test --locked -p aegis-model --test store
ok   cargo test --locked -p aegis-client --test undo_every_op
