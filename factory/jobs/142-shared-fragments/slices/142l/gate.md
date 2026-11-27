GATE: PASS
Slice: 142l  Base: 4b73f3764ca87c216443c05618f78fe397eae406


Diff: 245 counted lines, 23 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-client --test shared_fragments --test table_conventions --test table_features --test explorer --test grid_edit
ok   cargo test --locked -p aegis-workbench --test table_view
ok   cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace
ok   cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded --test relations --test engine_compatibility
