GATE: PASS
Slice: 136a  Base: 352f90be359287b7bc4e88d6fd6811cf8a331283


Diff: 622 counted lines, 14 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test assumptions
ok   cargo test --locked -p aegis-client --test undo_every_op
