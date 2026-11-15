GATE: PASS
Slice: 125b  Base: 6fcc1fe9a5bebbd793c7bdae60f083c2e39b4ce3


Diff: 186 counted lines, 2 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test reduced_by --test pages
ok   cargo check --locked -p aegis-shell
