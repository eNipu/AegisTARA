# Build Report — 040-password-login-and-grpc-web

## Summary
Adds server-managed password accounts (Argon2id 19 MiB/2 passes, `ats_` bearer tokens hashed with SHA-256, login throttling, user admin RPCs), dual HTTP/2 gRPC + HTTP/1.1 gRPC-Web transport with 20s stream heartbeats, `GET /healthz`, `PORT` support, client `Settings` persistence, and Railway deployment docs.

## Key Changes
- Design documented in `docs/adr/0035-password-accounts-and-admin.md`.
- Implemented across `crates/aegis-server/src/accounts`, `crates/aegis-server/src/server.rs`, `crates/aegis-client/src/transport.rs`, `docs/deploy-railway.md`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
