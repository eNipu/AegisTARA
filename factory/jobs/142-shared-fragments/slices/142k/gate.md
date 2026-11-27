GATE: PASS
Slice: 142k  Base: e9f51b4b0148f0ec0052cc2a13be9f5ee51cdcf7


Diff: 196 counted lines, 7 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded
