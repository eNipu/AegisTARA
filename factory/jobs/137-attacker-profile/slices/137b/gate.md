GATE: PASS
Slice: 137b  Base: 86c0746eb4f63e8a50ce16bf62bbc53a0fac2705


Diff: 283 counted lines, 18 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test attacker_profile --test applicability_cache --test threat_scaling
ok   cargo bench --locked -p aegis-model --bench applicability -- --test
ok   cargo test --locked -p aegis-client --test attacker_profile --test excluded_reasons --test condition_toast --test undo_every_op --test undo_labels
