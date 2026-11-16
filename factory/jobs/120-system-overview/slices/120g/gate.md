GATE: PASS
Slice: 120g  Base: c2202b5ded72ae14a8d62749ea4114971bc9dfb7


Diff: 177 counted lines, 3 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test combined --test systems --test pages --test session
ok   cargo check --locked -p aegis-shell
