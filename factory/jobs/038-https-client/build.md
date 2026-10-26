# Build Report — 038-https-client

## Summary
Replaces the stub HTTP client in `aegis-auth` with `HyperClient` over `hyper-rustls` (`ring` provider) and `rustls-native-certs`, enforcing HTTPS-only (except loopback for tests), no redirects, 10s request timeout, and 1 MiB response cap, tested against a TLS mock OIDC provider.

## Key Changes
- Design documented in `docs/adr/0028-oidc-dpop-auth.md`.
- Implemented across `crates/aegis-auth/src/http.rs`, `crates/aegis-auth/src/tls.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
