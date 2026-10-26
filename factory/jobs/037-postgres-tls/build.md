# Build Report — 037-postgres-tls

## Summary
Adds `rustls` (`ring` crypto provider) and `rustls-native-certs` TLS support to `PostgresStore` and `PostgresAccounts`, supporting `disable`, `encrypt` (`sslmode=require`), and `verify` (`sslmode=verify-full`) modes and verifying all dependencies with `cargo-deny`.

## Key Changes
- Design documented in `docs/adr/0027-postgres-project-store.md`.
- Implemented across `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/main.rs`, `deny.toml`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
