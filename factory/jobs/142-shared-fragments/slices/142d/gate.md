GATE: PASS
Slice: 142d  Base: 4ea7108


Diff: 287 counted lines, 5 files
ok   scripts/ci.sh crates/aegis-model
ok   cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace
ok   cargo test --locked -p aegis-client --test engine_compatibility
ok   cargo test --locked -p aegis-model --lib compat::tests
