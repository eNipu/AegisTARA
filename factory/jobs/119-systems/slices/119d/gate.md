GATE: PASS
Slice: 119d  Base: 4a62bfc366010e5482419fed80f19738a5677226


Diff: 40 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test systems --test ops_properties
ok   cargo test --locked -p aegis-client --test systems --test pages
