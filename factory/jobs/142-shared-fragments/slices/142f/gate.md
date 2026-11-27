GATE: PASS
Slice: 142f  Base: 38c3033


Diff: 192 counted lines, 11 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded --test relations --test inspector_view
