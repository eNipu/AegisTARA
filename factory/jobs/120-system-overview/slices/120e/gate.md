GATE: PASS
Slice: 120e  Base: 2d3cc15ebb55a798e2eef516e24aea39bede796d


Diff: 189 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-client
ok   cargo test --locked -p aegis-client --test combined --test combined_properties --test pages --test systems_view --test session
ok   cargo check --locked -p aegis-shell
