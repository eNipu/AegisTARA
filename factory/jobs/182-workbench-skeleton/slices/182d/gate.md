GATE: PASS
Slice: 182d  Base: 04fbe75238bc441bd4e966f29329a6eb17c606ac


Diff: 267 counted lines, 10 files
ok   scripts/ci.sh crates/aegis-workbench crates/aegis-client
ok   cargo test --locked -p aegis-workbench
ok   cargo test --locked -p aegis-client --test settings_layout
ok   git diff --quiet factory/182d-full HEAD -- crates/aegis-client/src/settings.rs crates/aegis-client/tests/settings_layout.rs crates/aegis-workbench/src/lib.rs crates/aegis-workbench/src/local.rs crates/aegis-workbench/tests/local.rs crates/aegis-workbench/src/app/layout.rs crates/aegis-workbench/tests/window.rs
