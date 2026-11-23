GATE: PASS
Slice: 135a  Base: 176663a559db52f0b05118f015cd7bf9daec9e06


Diff: 254 counted lines, 12 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test configuration
ok   cargo test --locked -p aegis-client --test process_map
ok   cargo check --locked -p aegis-shell -p aegis-workbench --tests
