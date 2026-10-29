# ADR 0054: Phase 5 security hardening sweep

- Status: accepted (jobs 060-login-hardening, 061-database-hardening, 062-event-log-integrity, 063-small-hardening)
- Date: 2026-10-28
- Scope: `aegis-auth` (`dpop.rs`, `pkce.rs`, `oidc.rs`), `aegis-server` (`accounts/`, `store/postgres.rs`, `store/audit.rs`, `service.rs`, `server.rs`, `main.rs`, `migrations/004_event_chain.sql`), `aegis-client` (`settings.rs`, `session.rs`)
- Rules: R3, R4, R7, P1, P4 in `docs/engineering-standards.md`
- Requirements: SEC-01, SEC-03, SEC-04, SEC-05, SEC-06, SEC-07, SEC-08, SEC-09, SEC-10
- Builds on: `0025-auth-and-rbac.md`, `0034-oidc-and-session-tokens.md`, `0035-postgres-store.md`, `0042-admin-audit-and-retention.md`

## Context

Phase 5 security review identified nine concrete hardening requirements (`SEC-01`, `SEC-03`–`SEC-10`) across login throttling, DPoP replay caching, PostgreSQL TLS verification, account store atomicity, tamper-evident audit logging, secret zeroization, private client file permissions, and HTTP/2 resource limits.

## Decisions

### 1. Login throttling, Argon2id admission control, and DPoP replay eviction (`060-login-hardening`: `SEC-01`, `SEC-03`, `SEC-04`)

- **Single-entry eviction in `Throttle` (`SEC-01`)**: `Throttle::fail` never clears the entire map on capacity (`MAX_TRACKED = 10_000`). Instead, it sweeps expired entries, and if still full when inserting a new name, evicts a single entry—preferring an unknown username over a known username (`is_known`), and an unblocked entry over an actively blocked one. Syntactically invalid usernames are rejected before consulting `Throttle` or acquiring an Argon2id hashing permit.
- **Per-address login admission and separate authenticated hashing pool (`SEC-04`)**: `AccountService` places `AddrThrottle` (`max_login_in_flight_per_addr: 2`, `free_addr_failures: 20`) in front of the `LogIn` Argon2id semaphore so a single peer IP cannot saturate global password-hashing permits or bypass per-address backoff. Authenticated operations (`SetPassword`, `CreateUser`, `RotateRefresh`) draw from a dedicated `auth_hashing` semaphore so unauthenticated login floods cannot starve authenticated users.
- **Monotonic DPoP `ReplayCache` eviction (`SEC-03`)**: `ReplayCache::check_and_insert` computes entry expiry monotonically from server arrival time (`now.saturating_add(self.window).saturating_add(self.skew)`), sweeps expired entries before capacity eviction, and therefore prevents a future-skewed `iat` proof at the front of the FIFO queue from blocking eviction of later proofs.

### 2. Verified PostgreSQL TLS and transactional account mutations (`061-database-hardening`: `SEC-05`, `SEC-07`)

- **Always verify PostgreSQL server certificates (`SEC-05`)**: Removed the custom `AnyCertificate` (`danger::ServerCertVerifier`) implementation from `crates/aegis-server/src/store/postgres.rs`. Added `ca_file: Option<PathBuf>` to `PostgresOptions` (wired via `AEGIS_DB_CA_FILE` in `aegis-server` and capped at `MAX_CA_FILE_BYTES = 1 MiB`). Both `DbTls::Encrypt` and `DbTls::Verify` verify the server's certificate chain and hostname against either the configured `ca_file` root store or the OS root store (`rustls_native_certs`).
- **Transactional multi-table account writes (`SEC-07`)**: Added `put_login_session`, `put_user_and_delete_sessions`, and `put_password_login_and_delete_sessions` to `AccountBackend`, `MemoryAccounts`, and `PostgresAccounts` (`client.transaction().await`). Disabling a user, rotating a password, or recording a login + session now commits atomically in PostgreSQL and rolls back in-memory state if the backend transaction fails.

### 3. Hash-chained tamper-evident server event log (`062-event-log-integrity`: `SEC-06`)

- **SHA-256 hash chain over `server_events`**: Migration `004_event_chain.sql` adds `prev_hash BYTEA NOT NULL` and `event_hash BYTEA NOT NULL` to `server_events`. Each event's hash is `event_link(&prev_hash, seq, at_ms, actor, kind, target, detail)` using domain-separated length-prefixed SHA-256 (`aegis-event-v1`), starting from `event_genesis()` (`SHA-256("aegis-event-v1\0genesis")`).
- **Chain continuity across retention trims**: `PostgresAccounts::trim_events` and `MemoryAccounts::trim_events` run inside a transaction that deletes rows older than the cutoff and, when any rows are deleted, appends an `AuditKind::EventsTrimmed` event chaining from the previous tail hash and recording the deleted row count and new first remaining sequence number. `EventChainVerifier` and `AegisService::verify_audit` verify the entire retained chain in streaming batches.
- **Bounded retry queue**: `EventLog` holds a bounded pending queue (`MAX_PENDING_EVENTS = 4096`, `R4`) so transient backend failures do not drop audit events silently, and flushes pending events in sequence order before appending new ones or verifying the chain.

### 4. Secret zeroization, private client files, and HTTP/2 limits (`063-small-hardening`: `SEC-08`, `SEC-09`, `SEC-10`)

- **Zeroize secrets on drop (`SEC-08`)**: Added volatile/black-box byte zeroization in `Drop` for `SecretBytes` in `dpop.rs`, `PkceVerifier` in `pkce.rs`, `ClientSecret` in `oidc.rs`, `Password` in `accounts/mod.rs`, and temporary password-hash buffers in `accounts/password.rs`.
- **Private file creation (`0600`) and atomic overwrite (`SEC-09`)**: Added `write_private_file` in `crates/aegis-client/src/settings.rs` using `OpenOptions::create_new(true)` with `mode(0o600)` on Unix, symlink rejection on the destination path, and atomic rename from a unique sibling temporary file. Used by `Settings::save` and export file writers in `session.rs`.
- **Explicit HTTP/2 server limits (`SEC-10`)**: Configured `tonic::transport::Server::builder()` in `crates/aegis-server/src/server.rs` with `http2_max_concurrent_streams(128)`, `initial_stream_window_size(1 MiB)`, `initial_connection_window_size(2 MiB)`, `max_frame_size(64 KiB)`, and `tcp_keepalive(60s)`.

## Consequences

- `DbTls::Encrypt` now requires a valid certificate chain (from OS roots or `AEGIS_DB_CA_FILE`), matching standard TLS expectations and eliminating MITM exposure on encrypted database connections.
- Audit log tampering (row modification, deletion, or reordering) is deterministically detected by `verify_audit`.
