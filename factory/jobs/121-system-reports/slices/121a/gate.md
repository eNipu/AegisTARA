GATE: PASS
Slice: 121a  Base: 37f8f710d9d87f697cbc186216a21587acb2898f


Diff: 266 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test system_reports --test report_builder --test reduced_by_report
ok   cargo check --locked -p aegis-shell
