GATE: PASS
Slice: 121c  Base: 5b11bfb7105efa082c1e9ad0fcb0ed99bc48e39f


Diff: 56 counted lines, 2 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-client --test system_reports --test report_builder --test reduced_by_report
ok   cargo test --locked -p aegis-client --lib excel
