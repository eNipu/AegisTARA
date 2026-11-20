GATE: PASS
Slice: 189c1a  Base: de2095a47c2c4811f2e4cf9c97c6f479b3f4b321


Diff: 347 counted lines, 12 files
ok   scripts/ci.sh crates/aegis-client crates/aegis-workbench
ok   cargo test --locked -p aegis-client --test command_table
ok   cargo test --locked -p aegis-client --test palette
ok   cargo test --locked -p aegis-workbench --test command_wiring
