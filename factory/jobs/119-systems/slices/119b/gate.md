GATE: PASS
Slice: 119b  Base: 7232db9690e387678da3b619ba2beea17c20597a


Diff: 208 counted lines, 13 files
ok   scripts/ci.sh crates/aegis-model crates/aegis-client
ok   cargo test --locked -p aegis-model --test systems --test ops_properties --test store --test workspace --test store_properties --test workspace_properties
ok   cargo test --locked -p aegis-client --test undo_every_op
