# Spec — 030-server-persistence

## Title
ProjectStore trait with in-memory and PostgreSQL backends and SHA-256 hash-chained op audit log

## Design Reference
- Reference: `docs/adr/0027-postgres-project-store.md`
- Affected crates/paths: `crates/aegis-server/src/store/mod.rs`, `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/store/audit.rs`

## Problem & Goal
Persists projects server-side via `ProjectStore` (in-memory `MemoryStore` and PostgreSQL `PostgresStore`) holding snapshots + append-only op logs, a cryptographic SHA-256 hash-chained audit record (`AEGIS-AUDIT-V1`) for every accepted op, and headless round-trip verification.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0027-postgres-project-store.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Parameterized SQL queries throughout `PostgresStore`, transactional `SELECT ... FOR UPDATE` on op append, and tamper-evident SHA-256 hash chaining across `(project_id, seq, timestamp_ms, actor, op, prev_hash)`.
3. UX invariants hold: Automatic schema initialization on startup and deterministic project rebuild from snapshot + trailing ops.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
