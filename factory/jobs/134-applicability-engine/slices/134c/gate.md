GATE: PASS
Slice: 134c  Base: 14d7573cdc3f9fa270608e01065d6165ea948253


Diff: 224 counted lines, 5 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-model --test applicability_cache
ok   cargo test --locked -p aegis-model --test ops_properties
ok   cargo bench --locked -p aegis-model --bench applicability
