GATE: PASS
Slice: 136b  Base: dc306bd132e5af64956a1bd481ccf36db03deb1f


Diff: 511 counted lines, 9 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test assumptions
ok   cargo test --locked -p aegis-client --test undo_every_op
