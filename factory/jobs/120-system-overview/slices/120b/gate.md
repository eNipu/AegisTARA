GATE: PASS
Slice: 120b  Base: 550d275140e2cc35ad85e932e12cf089fdc53f9e


Diff: 211 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test combined --test systems --test pages
ok   cargo check --locked -p aegis-shell
