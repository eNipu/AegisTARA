GATE: PASS
Slice: 134b  Base: 7197189b66074de7c57f34371411bfe8d2b4dbf4


Diff: 500 counted lines, 7 files
ok   scripts/ci.sh crates/aegis-model
ok   cargo test --locked -p aegis-model --test applicability_eval
ok   cargo test --locked -p aegis-model --test ops_properties
ok   cargo test --locked -p aegis-model --test project --test eval --test explain
