# Build Report — 066-i18n-foundation

## Summary


## Key Changes
- aegis-client/src/i18n.rs: Language enum, Fluent Catalog (fluent-bundle), locale detection, bidi-mark stripping. - locales/{en,ja,pseudo}/main.ftl. - aegis-shell: @tr on all static UI strings, build.rs bundles translations, launcher selects the language. - settings.rs language field + language(). - tests: crates/aegis-shell/tests/i18n.rs (missing-key), report_builder pseudo test.

## Verification
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `./scripts/deny.sh`: PASS
