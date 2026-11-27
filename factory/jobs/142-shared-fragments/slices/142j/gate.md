GATE: PASS
Slice: 142j  Base: 532863a0f072511a608348d99f76ef91077bcf06


Diff: 144 counted lines, 7 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded
