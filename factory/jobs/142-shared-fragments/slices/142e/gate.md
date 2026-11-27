GATE: PASS
Slice: 142e  Base: 5e124b3


Diff: 280 counted lines, 7 files
ok   scripts/ci.sh crates/aegis-model
ok   cargo test --locked -p aegis-model --test shared_fragments --test ops_properties
ok   cargo bench --locked -p aegis-model --bench eval -- fragment_single_edit_50
