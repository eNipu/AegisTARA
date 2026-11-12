GATE: PASS
Slice: 205d  Base: 4622528e1106b3f984cfa5b22199686c860305b8


Diff: 321 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test elements
ok   cargo test --locked -p aegis-client --test undo_every_op
