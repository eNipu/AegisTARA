GATE: PASS
Slice: 145c  Base: 189b2281c4798284c68dafe441c370090c389b44


Diff: 206 counted lines, 10 files
ok   scripts/ci.sh crates/aegis-model
ok   cargo test --locked -p aegis-model --test store
ok   cargo test --locked -p aegis-model --test store_limits
ok   cargo test --locked -p aegis-model --test party_lint
ok   cargo test --locked -p aegis-model --test ops_properties --test store_properties
