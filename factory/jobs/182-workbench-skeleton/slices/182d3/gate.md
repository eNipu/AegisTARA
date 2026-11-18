GATE: PASS
Slice: 182d3  Base: cdb8348e9ed0cc16a9c829ca2ddcf3d94469f777


Diff: 286 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-workbench
ok   cargo test --locked -p aegis-workbench
ok   git diff --quiet factory/182d-full HEAD
