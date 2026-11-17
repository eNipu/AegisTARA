GATE: PASS
Slice: 145a  Base: 7f83a51


Diff: 598 counted lines, 11 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --lib id
ok   cargo test --locked -p aegis-model --test parties
ok   cargo test --locked -p aegis-client --test undo_every_op
