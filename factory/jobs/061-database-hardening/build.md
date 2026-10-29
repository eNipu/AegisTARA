# Build Report — 061-database-hardening

## Summary
Removed `AnyCertificate` from `store/postgres.rs`, added `PostgresOptions.ca_file` (`AEGIS_DB_CA_FILE`), and made multi-table account/session writes transactional across `AccountBackend`, `PostgresAccounts`, `MemoryAccounts`, and `Accounts` (`ADR 0054`).

## Key Changes
- `crates/aegis-server/src/store/postgres.rs` & `src/main.rs`: Removed `AnyCertificate`, added `ca_file: Option<PathBuf>` and `load_ca_roots` (`MAX_CA_FILE_BYTES = 1 MiB`), and wired `AEGIS_DB_CA_FILE`.
- `crates/aegis-server/src/accounts/{mod,memory,postgres}.rs`: Added transactional `put_login_session`, `put_user_and_delete_sessions`, and `put_password_login_and_delete_sessions` with in-memory rollback on backend failure.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
