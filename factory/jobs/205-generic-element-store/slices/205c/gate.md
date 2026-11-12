GATE: PASS
Slice: 205c  Base: 88b9124b42314a8bae4618a7f423a14496885f57


Diff: 251 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test elements
ok   cargo test --locked -p aegis-client --test undo_every_op
