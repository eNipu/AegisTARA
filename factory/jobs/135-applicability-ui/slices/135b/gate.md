GATE: PASS
Slice: 135b  Base: 3b9abd6e45a69455c0eb7beef418c577a0f6d3f1


Diff: 246 counted lines, 7 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test excluded_reasons
ok   cargo test --locked -p aegis-client --test condition_toast
