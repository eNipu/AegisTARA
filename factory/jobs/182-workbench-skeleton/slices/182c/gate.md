GATE: PASS
Slice: 182c  Base: c886292b93c0baa1060c56a659129cb83371dc9f


Diff: 265 counted lines, 12 files
ok   scripts/ci.sh crates/aegis-workbench
ok   cargo build --locked -p aegis-workbench
ok   cargo test --locked -p aegis-workbench
ok   scripts/deny.sh licenses bans sources
ok   test -f crates/aegis-workbench/themes/light.json
ok   test -f crates/aegis-workbench/themes/dark.json
ok   git diff --quiet factory/182c-full HEAD -- Cargo.toml Cargo.lock deny.toml crates/aegis-workbench/Cargo.toml crates/aegis-workbench/src/theme.rs crates/aegis-workbench/src/sample.rs crates/aegis-workbench/themes crates/aegis-workbench/tests/theme.rs crates/aegis-workbench/tests/sample.rs
