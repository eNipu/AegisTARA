# Spec — 062-event-log-integrity

## Title
Event log integrity: SHA-256 hash chain over `server_events`, retention-trim events, bounded retry queue, and `VerifyAudit` coverage (`ADR 0054`, `SEC-06`)

## Design Reference
- Reference: `docs/adr/0054-security-hardening-sweep.md`
- Affected crates/paths: `crates/aegis-server/migrations/004_event_chain.sql`, `crates/aegis-server/src/store/{audit,postgres}.rs`, `crates/aegis-server/src/accounts/{mod,memory,postgres}.rs`, `crates/aegis-server/src/service.rs`

## Problem & Goal
Hash-chain `server_events` (`aegis-event-v1`) like the project operation audit log, record each retention trim (`AuditKind::EventsTrimmed`) inside the same transaction so the hash chain remains unbroken across trims, buffer events in a bounded retry queue (`MAX_PENDING_EVENTS = 4096`) on transient database errors instead of silently dropping them, and verify the event chain in `VerifyAudit` (`SEC-06`).

## Acceptance Criteria
1. Migration `004_event_chain.sql` adds `prev_hash BYTEA NOT NULL` and `event_hash BYTEA NOT NULL` to `server_events`.
2. `event_genesis`, `event_link`, and `EventChainVerifier` compute and verify the SHA-256 chain (`aegis-event-v1`), allowing continuity across `AuditKind::EventsTrimmed`.
3. `EventLog` queues events on transient backend failures up to `MAX_PENDING_EVENTS = 4096`, surfaces errors via `try_record` / `flush`, and `AegisService::verify_audit` verifies both the project op chain and the server event chain.
