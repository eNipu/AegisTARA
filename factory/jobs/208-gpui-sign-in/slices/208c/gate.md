GATE: PASS
Slice: 208c  Base: 3b6e4bad1224c8d7bc14f4c69401af04abf7c676


Diff: 268 counted lines, 12 files
ok   scripts/ci.sh crates/aegis-client crates/aegis-workbench
ok   cargo test --locked -p aegis-client --test license_file
ok   cargo test --locked -p aegis-client --test session snapshot
ok   cargo test --locked -p aegis-workbench --test read_only
ok   cargo test --locked -p aegis-workbench --test folder_view
