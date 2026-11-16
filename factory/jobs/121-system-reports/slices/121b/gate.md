GATE: PASS
Slice: 121b  Base: 1bcd981c6a4e93e477efc0df2755331663250b9c


Diff: 219 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test system_reports --test report_builder --test reduced_by_report --test pages
ok   cargo check --locked -p aegis-shell
