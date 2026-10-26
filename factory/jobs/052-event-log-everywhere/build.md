# Build Report — 052-event-log-everywhere

## Summary
Ensures the persistent security event log (`EventLog`, `aegis_event`) runs on every server mode (company-login-only, password, hybrid, and dev), enforcing retention by both row count (`max_events = 10,000`) and age (`retention = 90 days`), verified by a high-volume stress test.

## Key Changes
- Design documented in `docs/adr/0040-audit-event-log.md`.
- Implemented across `crates/aegis-server/src/accounts/events.rs`, `crates/aegis-server/src/accounts/postgres.rs`, `crates/aegis-server/src/service.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
