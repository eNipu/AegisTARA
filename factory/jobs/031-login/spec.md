# Spec — 031-login

## Title
OIDC authorization code flow with PKCE, loopback redirect (RFC 8252), DPoP (RFC 9449), and OS keychain

## Design Reference
- Reference: `docs/adr/0028-oidc-dpop-auth.md`
- Affected crates/paths: `crates/aegis-auth`, `crates/aegis-server/src/auth_layer.rs`

## Problem & Goal
Creates `crates/aegis-auth`: OIDC authorization code flow with S256 PKCE and `127.0.0.1:0` loopback redirect (RFC 8252), ES256 DPoP sender-constrained proofs (RFC 9449), `SecretStore` with OS keychain backend (`keyring`), and server-side JWT + DPoP proof validation (`Authenticator`, `ReplayCache`).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0028-oidc-dpop-auth.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Strict JOSE algorithm allowlist (`ES256`, `RS256`), constant-time OAuth `state` check, DPoP `ath` + `cnf.jkt` binding, bounded `ReplayCache`, and OS keychain storage for private device key and refresh token.
3. UX invariants hold: Seamless system-browser SSO sign-in with automatic token refresh.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
