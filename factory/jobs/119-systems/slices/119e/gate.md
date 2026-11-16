GATE: PASS
Slice: 119e  Base: 4d0e2d7235985ca7cc0d6a1cc0f09e26f1d89779


Diff: 191 counted lines, 3 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test systems --test undo_labels --test pages
ok   cargo check --locked -p aegis-shell
