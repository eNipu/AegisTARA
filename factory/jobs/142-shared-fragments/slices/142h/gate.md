GATE: PASS
Slice: 142h  Base: ef86fe981b6e18ffa8c4fc26e020a1471bae5fb7


Diff: 288 counted lines, 13 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace
ok   cargo test --locked -p aegis-client --test undo_every_op --test engine_compatibility
ok   cargo test --locked -p aegis-model --lib compat::tests
