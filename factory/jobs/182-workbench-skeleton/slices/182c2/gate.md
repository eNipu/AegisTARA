GATE: PASS
Slice: 182c2  Base: 4218fc80c68e630a1c9b1698e4d207741932f478


Diff: 436 counted lines, 9 files
ok   scripts/ci.sh crates/aegis-workbench crates/aegis-client
ok   cargo build --locked -p aegis-workbench
ok   cargo test --locked -p aegis-workbench
ok   git diff --quiet factory/182c-full HEAD
