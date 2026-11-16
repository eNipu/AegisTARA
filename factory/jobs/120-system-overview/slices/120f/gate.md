GATE: PASS
Slice: 120f  Base: e68be9fbdf8f2e2dce815b7f94a3e1c94fcda3fb


Diff: 208 counted lines, 4 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test top_paths --test combined --test pages
ok   cargo check --locked -p aegis-shell
