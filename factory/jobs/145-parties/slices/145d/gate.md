GATE: PASS
Slice: 145d  Base: 726042f2d4e7889932c67bf54e489259514063b1


Diff: 338 counted lines, 15 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test relations
ok   cargo test --locked -p aegis-client --test table_conventions
ok   cargo test --locked -p aegis-client --lib relations
ok   cargo test --locked -p aegis-client --test undo_labels
