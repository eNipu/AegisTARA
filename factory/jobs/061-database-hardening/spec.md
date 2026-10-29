# Spec — 061-database-hardening

## Title
Database hardening: verified PostgreSQL TLS (`SEC-05`) and transactional multi-table account/session mutations (`SEC-07`) (`ADR 0054`)

## Design Reference
- Reference: `docs/adr/0054-security-hardening-sweep.md`
- Affected crates/paths: `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/main.rs`, `crates/aegis-server/src/accounts/{mod,memory,postgres}.rs`, `crates/aegis-server/tests/postgres.rs`

## Problem & Goal
1. `SEC-05`: Remove the custom `AnyCertificate` verifier from `DbTls::Encrypt` so both `DbTls::Encrypt` and `DbTls::Verify` verify the PostgreSQL server's certificate chain and hostname against OS roots or a configured `AEGIS_DB_CA_FILE` (`PostgresOptions.ca_file`).
2. `SEC-07`: Run every multi-step account and session mutation (login session + `last_login_ms`, user update/disable + session revocation, password login switch + session revocation) inside a single database transaction with in-memory rollback on failure.

## Acceptance Criteria
1. `AnyCertificate` (`danger::ServerCertVerifier`) is removed from `crates/aegis-server/src/store/postgres.rs`; `PostgresOptions.ca_file` loads custom PEM roots (capped at `MAX_CA_FILE_BYTES = 1 MiB`, `R3`).
2. `put_login_session`, `put_user_and_delete_sessions`, and `put_password_login_and_delete_sessions` execute atomically in `PostgresAccounts` (`client.transaction().await`) and `MemoryAccounts`, and `Accounts` restores in-memory state if the backend write fails.
3. Unit and integration tests verify certificate rejection without trusted CA roots and atomic multi-table updates.
