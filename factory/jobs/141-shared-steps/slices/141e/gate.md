GATE: PASS
Slice: 141e  Base: 284d1d41c5ec7d0d94942c2fc485c20986045d39


Diff: 700 counted lines, 22 files
ok   scripts/ci.sh crates/aegis-client crates/aegis-workbench
ok   cargo test --locked -p aegis-client --test shared_steps --test relations --test inspector_view --test undo_every_op
ok   cargo test --locked -p aegis-workbench --test inspector_view
