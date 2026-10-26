# Spec — 038-https-client

## Title
Rustls + OS trust store HTTPS client (`HyperClient`) for OIDC login and JWKS

## Design Reference
- Reference: `docs/adr/0028-oidc-dpop-auth.md`
- Affected crates/paths: `crates/aegis-auth/src/http.rs`, `crates/aegis-auth/src/tls.rs`

## Problem & Goal
Replaces the stub HTTP client in `aegis-auth` with `HyperClient` over `hyper-rustls` (`ring` provider) and `rustls-native-certs`, enforcing HTTPS-only (except loopback for tests), no redirects, 10s request timeout, and 1 MiB response cap, tested against a TLS mock OIDC provider.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0028-oidc-dpop-auth.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Validates server TLS certificates against the OS trust store (`rustls-native-certs`), forbids HTTP redirects, and enforces strict time and byte caps (R3, R6).
3. UX invariants hold: Works transparently with corporate roots installed in the OS certificate store.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
