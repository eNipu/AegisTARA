# Spec — 055-session-admin

## Title
Admin active password session listing (`ListSessions`) and immediate revocation (`RevokeSession`)

## Design Reference
- Reference: `docs/adr/0049-session-admin.md`
- Affected crates/paths: `crates/aegis-server/src/accounts/mod.rs`, `crates/aegis-proto/proto/aegis/v1/tara.proto`, `crates/aegis-client`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Adds `ListSessions` (newest first, capped at `MAX_SESSION_LIST = 500`, identifying sessions by the 16-hex-char 64-bit prefix `SessionKey::id()` of `SHA-256(token)`) and `RevokeSession` (`revoke_session` + `session_revoked` audit event), plus a 100,000-session stress test.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0049-session-admin.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Never exposes raw bearer tokens; `revoke_session` removes the session from memory and PostgreSQL (rolling back memory on DB fault) so the next request with that token is immediately rejected.
3. UX invariants hold: Administrators can inspect and terminate individual password sessions by device/client name without disabling the user's account.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
