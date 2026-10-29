# Build Report — 063-small-hardening

## Summary
Implemented `SEC-08` (zeroize-on-drop for secrets and passwords), `SEC-09` (`write_private_file` with `create_new`, `0600`, and symlink rejection for settings and exports), and `SEC-10` (explicit HTTP/2 and TCP keepalive limits in `server.rs`), documented in ADR 0054 (`docs/adr/0054-security-hardening-sweep.md`).

## Key Changes
- `crates/aegis-auth/src/{dpop,pkce,oidc}.rs` & `crates/aegis-server/src/accounts/{mod,password}.rs`: Added zeroize-on-drop via `std::hint::black_box`.
- `crates/aegis-client/src/{settings,session}.rs`: Added `write_private_file` (`create_new(true)`, `0o600` mode on Unix, symlink check, atomic rename) and used it for `Settings::save` and export files.
- `crates/aegis-server/src/server.rs`: Configured HTTP/2 stream, window, frame, and keepalive limits on `Server::builder()`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
