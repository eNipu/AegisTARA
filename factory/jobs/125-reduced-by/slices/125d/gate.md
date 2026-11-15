GATE: PASS
Slice: 125d  Base: 425a9180639a0f0874b1deb5c10cfd8117d177d9


Diff: 40 counted lines, 2 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-client --test reduced_by_report --test report_builder --test pages
ok   cargo check --locked -p aegis-shell
