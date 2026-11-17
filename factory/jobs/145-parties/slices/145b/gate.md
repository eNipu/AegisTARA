GATE: PASS
Slice: 145b  Base: 346c4b58a548766793284af2e25f9e7d7d8e7d9b


Diff: 507 counted lines, 10 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test parties
ok   cargo test --locked -p aegis-model --test store
ok   cargo test --locked -p aegis-client --test undo_every_op
