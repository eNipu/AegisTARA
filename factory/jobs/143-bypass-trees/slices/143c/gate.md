GATE: PASS
Slice: 143c  Base: be419820e37cffa94a28f1a31367355dbe64a4a6


Diff: 277 counted lines, 18 files
ok   scripts/ci.sh crates/aegis-canvas crates/aegis-client
ok   cargo test --locked -p aegis-canvas
ok   cargo test --locked -p aegis-client --test relations --test inspector_view --test excluded_canvas --test undo_every_op --test reduced_by --test bypass_trees
