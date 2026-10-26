# Build Report — 030-server-persistence

## Summary
Persists projects server-side via `ProjectStore` (in-memory `MemoryStore` and PostgreSQL `PostgresStore`) holding snapshots + append-only op logs, a cryptographic SHA-256 hash-chained audit record (`AEGIS-AUDIT-V1`) for every accepted op, and headless round-trip verification.

## Key Changes
- Design documented in `docs/adr/0027-postgres-project-store.md`.
- Implemented across `crates/aegis-server/src/store/mod.rs`, `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/store/audit.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
