# Build Report — 045-password-login-switch

## Summary
Allows administrators on a hybrid (`oidc+password`) server to turn password login off or on at runtime (`SetPasswordLogin`), persisting the switch in `aegis_setting`, immediately terminating all open password sessions when turned off, and hiding the password login option in the client.

## Key Changes
- Design documented in `docs/adr/0039-password-login-switch.md`.
- Implemented across `crates/aegis-server/src/accounts/mod.rs`, `crates/aegis-client`, `crates/aegis-shell`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
