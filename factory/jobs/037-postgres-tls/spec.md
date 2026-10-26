# Spec — 037-postgres-tls

## Title
Rustls-based PostgreSQL TLS (`tokio-postgres-rustls`) with `sslmode` / `AEGIS_DB_TLS` support

## Design Reference
- Reference: `docs/adr/0027-postgres-project-store.md`
- Affected crates/paths: `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/main.rs`, `deny.toml`

## Problem & Goal
Adds `rustls` (`ring` crypto provider) and `rustls-native-certs` TLS support to `PostgresStore` and `PostgresAccounts`, supporting `disable`, `encrypt` (`sslmode=require`), and `verify` (`sslmode=verify-full`) modes and verifying all dependencies with `cargo-deny`.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0027-postgres-project-store.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Uses pure-Rust/ring `rustls` (no OpenSSL dependency) and OS trust roots for `DbTls::Verify`. (See `docs/security-and-architecture-review.md` SEC-05 for `AEGIS_DB_CA_FILE` recommendation).
3. UX invariants hold: Supports both standard `sslmode=` query parameters in `AEGIS_DATABASE_URL` and explicit `AEGIS_DB_TLS` overrides.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
