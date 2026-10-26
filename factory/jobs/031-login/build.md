# Build Report — 031-login

## Summary
Creates `crates/aegis-auth`: OIDC authorization code flow with S256 PKCE and `127.0.0.1:0` loopback redirect (RFC 8252), ES256 DPoP sender-constrained proofs (RFC 9449), `SecretStore` with OS keychain backend (`keyring`), and server-side JWT + DPoP proof validation (`Authenticator`, `ReplayCache`).

## Key Changes
- Design documented in `docs/adr/0028-oidc-dpop-auth.md`.
- Implemented across `crates/aegis-auth`, `crates/aegis-server/src/auth_layer.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
