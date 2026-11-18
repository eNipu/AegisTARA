GATE: PASS
Slice: 182e  Base: 52ecdb0934317a6b706058d31392be2a7228b5d1


Diff: 478 counted lines, 8 files
ok   scripts/ci.sh crates/aegis-workbench
ok   cargo test --locked -p aegis-workbench
ok   git diff --quiet factory/182e-full HEAD -- crates/aegis-workbench/src/app/commands.rs crates/aegis-workbench/src/app/keys.rs crates/aegis-workbench/src/app/icons.rs crates/aegis-workbench/src/app/mod.rs crates/aegis-workbench/tests/command_wiring.rs crates/aegis-workbench/tests/explorer_view.rs
