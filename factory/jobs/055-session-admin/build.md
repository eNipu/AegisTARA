# Build Report — 055-session-admin

## Summary
Adds `ListSessions` (newest first, capped at `MAX_SESSION_LIST = 500`, identifying sessions by the 16-hex-char 64-bit prefix `SessionKey::id()` of `SHA-256(token)`) and `RevokeSession` (`revoke_session` + `session_revoked` audit event), plus a 100,000-session stress test.

## Key Changes
- Design documented in `docs/adr/0049-session-admin.md`.
- Implemented across `crates/aegis-server/src/accounts/mod.rs`, `crates/aegis-proto/proto/aegis/v1/tara.proto`, `crates/aegis-client`, `crates/aegis-shell/ui/app.slint`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
