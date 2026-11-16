GATE: PASS
Slice: 119f  Base: 9572b7089c66c3e7e81de39a8b7a51b14bedfe28


Diff: 227 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test systems --test pages --test session
ok   cargo check --locked -p aegis-shell
