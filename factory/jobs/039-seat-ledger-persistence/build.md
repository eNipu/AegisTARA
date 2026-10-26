# Build Report — 039-seat-ledger-persistence

## Summary
Rebuilds the active editor seat table (`SeatTable`) on server startup from recent project operation timestamps (`store.recent_editors`), ensuring a server restart does not reset active seat counts.

## Key Changes
- Design documented in `docs/adr/0033-license-and-seats.md`.
- Implemented across `crates/aegis-server/src/license.rs`, `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/main.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
