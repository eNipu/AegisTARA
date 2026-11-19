GATE: PASS
Slice: 208a  Base: ae9510a40d5ac861ae45ba3d909a99433cbfd09c


Diff: 280 counted lines, 11 files
ok   scripts/ci.sh crates/aegis-client crates/aegis-shell
ok   cargo test --locked -p aegis-client --test signin_view
ok   cargo build --locked -p aegis-shell
