GATE: PASS
Slice: 120d  Base: db8b3ff236a8e6f1edfabf8bcce7b2f29c9cd3cb


Diff: 169 counted lines, 4 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test combined --test combined_properties --test pages
ok   cargo check --locked -p aegis-shell
