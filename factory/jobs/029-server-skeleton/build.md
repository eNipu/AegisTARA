# Build Report — 029-server-skeleton

## Summary
Adds `crates/aegis-proto` and `crates/aegis-server`: Tonic gRPC service validating and applying `Op`s through `aegis-model` with one actor task per open project (rule C1), 2-minute exclusive edit lease (`LeaseState`, D4), bounded observer streams (rule R4), and decode caps at the API boundary (rule R3).

## Key Changes
- Design documented in `docs/adr/0026-server-rpc-and-actor-core.md`.
- Implemented across `crates/aegis-proto`, `crates/aegis-server`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
