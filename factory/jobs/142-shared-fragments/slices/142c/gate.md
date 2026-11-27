GATE: PASS
Slice: 142c  Base: 40fe73d97a91132f07a951cd2f6753b74cf40d8a


Diff: 301 counted lines, 17 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace
ok   cargo test --locked -p aegis-client --test undo_every_op --test engine_compatibility
ok   cargo test --locked -p aegis-model --lib compat::tests
