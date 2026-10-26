# Build Report — 036-release-scaffolding

## Summary
Adds release profile (`lto = "thin"`, `strip = "symbols"`, `codegen-units = 1`), multi-stage `Containerfile` (non-root UID 10001 `aegis`), `deploy/compose.test.yaml`, and Ed25519-signed update manifest verification (`AEGIS-UPDATE-V1`) in `aegis-license` and `aegis-client`.

## Key Changes
- Design documented in `docs/adr/0034-signed-update-manifest.md`.
- Implemented across `Cargo.toml`, `Containerfile`, `deploy/compose.test.yaml`, `crates/aegis-license/src/manifest.rs`, `crates/aegis-client/src/updates.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
