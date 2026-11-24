GATE: PASS
Slice: 137c  Base: 8ed9e95e596798f0e8ddf12484fe068f4a5d8162


Diff: 257 counted lines, 17 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-client --test attacker_profile --test configuration --test report_configuration --test report_builder --test combined
ok   cargo test --locked -p aegis-shell --test nav_groups --test standalone --test ui --test i18n
