# Spec — 040-password-login-and-grpc-web

## Title
Argon2id password accounts, session tokens, gRPC-Web transport, `/healthz`, and Railway deployment

## Design Reference
- Reference: `docs/adr/0035-password-accounts-and-admin.md`
- Affected crates/paths: `crates/aegis-server/src/accounts`, `crates/aegis-server/src/server.rs`, `crates/aegis-client/src/transport.rs`, `docs/deploy-railway.md`

## Problem & Goal
Adds server-managed password accounts (Argon2id 19 MiB/2 passes, `ats_` bearer tokens hashed with SHA-256, login throttling, user admin RPCs), dual HTTP/2 gRPC + HTTP/1.1 gRPC-Web transport with 20s stream heartbeats, `GET /healthz`, `PORT` support, client `Settings` persistence, and Railway deployment docs.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0035-password-accounts-and-admin.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Stores only `SHA-256(token)` in memory and PostgreSQL, runs Argon2id on `spawn_blocking` behind a bounded semaphore, and uses constant-time verification with decoy hashes for unknown users. (See `docs/security-and-architecture-review.md` SEC-01 and SEC-04 for hardening recommendations).
3. UX invariants hold: Auto-selects gRPC-Web for `https://` addresses to traverse HTTP/1.1 proxies and discovers server login options automatically.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
