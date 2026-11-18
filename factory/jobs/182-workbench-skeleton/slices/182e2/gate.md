GATE: PASS
Slice: 182e2  Base: 2f5401ad96202f318c51b280a16b3dc2101a9648


Diff: 287 counted lines, 3 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-workbench
ok   scripts/deny.sh licenses bans sources
ok   git diff --quiet factory/182e-full HEAD
