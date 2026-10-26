# Build Report — 035-live-observers

## Summary
Streams accepted operations, lock-holder transitions, and recomputed risk results from the server's project actor to all subscribed read-only clients (`Watch` RPC), with automatic resync when an observer falls behind the in-memory backlog ring.

## Key Changes
- Design documented in `docs/adr/0030-client-core.md`.
- Implemented across `crates/aegis-server/src/actor.rs`, `crates/aegis-client/src/session.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
