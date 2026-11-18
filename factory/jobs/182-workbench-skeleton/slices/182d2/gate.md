GATE: PASS
Slice: 182d2  Base: d7ab931558313fc95f533fc86ff426ec0b999e17


Diff: 423 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-workbench crates/aegis-client
ok   cargo test --locked -p aegis-workbench
ok   git diff --quiet factory/182d-full HEAD -- crates/aegis-workbench/src/app/icons.rs crates/aegis-workbench/src/app/layout.rs crates/aegis-client/locales crates/aegis-client/src/settings.rs crates/aegis-workbench/src/local.rs
