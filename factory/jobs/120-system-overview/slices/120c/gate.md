GATE: PASS
Slice: 120c  Base: 364578ce3943bfe6d22d720df6ec7d7f7c1d9d10


Diff: 51 counted lines, 2 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test combined --test step_via --test view
ok   cargo check --locked -p aegis-shell
