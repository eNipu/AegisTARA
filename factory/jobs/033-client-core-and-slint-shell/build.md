# Build Report — 033-client-core-and-slint-shell

## Summary
Builds `crates/aegis-client` (single-actor `Session`, `Workspace` with optimistic edits and inverse-op undo/redo over `ProjectEvalCache`, lease renewal, and `ViewModel` snapshots) and `crates/aegis-shell` (Slint desktop UI, headless-tested on the software renderer, D7).

## Key Changes
- Design documented in `docs/adr/0030-client-core.md`.
- Implemented across `crates/aegis-client`, `crates/aegis-shell`, `deny.toml`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
