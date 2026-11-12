GATE: PASS
Slice: 205b  Base: 82111d5d6e02884e84fc7921d468cfe49554b757


Diff: 475 counted lines, 11 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test elements
ok   cargo test --locked -p aegis-client --test undo_every_op
