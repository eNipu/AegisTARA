GATE: PASS
Slice: 136d  Base: 9f93d93d645f64d5124c2f31b7be09376437a306


Diff: 567 counted lines, 26 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench
ok   cargo test --locked -p aegis-model --test assumptions
ok   cargo test --locked -p aegis-client --test relations
ok   cargo test --locked -p aegis-client --test inspector_view
ok   cargo test --locked -p aegis-client --test table_conventions
