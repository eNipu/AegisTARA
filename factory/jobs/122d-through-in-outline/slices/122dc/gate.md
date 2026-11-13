GATE: PASS
Slice: 122dc  Base: efcc00aa143c1543f7943c56079f10968bc26561


Diff: 71 counted lines, 11 files
ok   scripts/ci.sh
ok   cargo test --locked -p aegis-model --test library
ok   cargo test --locked -p aegis-client --test library_insert --test library_view
ok   cargo check --locked --all-targets --manifest-path tools/aegis-ai/Cargo.toml
