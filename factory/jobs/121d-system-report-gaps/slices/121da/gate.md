GATE: PASS
Slice: 121da  Base: e7b38ffec55e50e28169488b3abdccf6212f56d9


Diff: 93 counted lines, 7 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test system_reports --test report_builder --test reduced_by_report
ok   cargo test --locked -p aegis-client --test session a_stale_system_name
ok   cargo check --locked -p aegis-shell
