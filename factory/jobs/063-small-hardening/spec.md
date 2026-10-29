# Spec — 063-small-hardening

## Title
Small hardening: secret zeroization (`SEC-08`), private file permissions (`SEC-09`), and explicit HTTP/2 server limits (`SEC-10`) (`ADR 0054`)

## Design Reference
- Reference: `docs/adr/0054-security-hardening-sweep.md`
- Affected crates/paths: `crates/aegis-auth/src/{dpop,pkce,oidc}.rs`, `crates/aegis-server/src/accounts/{mod,password}.rs`, `crates/aegis-client/src/{settings,session}.rs`, `crates/aegis-server/src/server.rs`

## Problem & Goal
1. `SEC-08`: Zeroize secret key material, PKCE verifiers, client secrets, passwords, and temporary hash buffers on drop.
2. `SEC-09`: Create settings and export files with `create_new(true)`, owner-only permissions (`0600` on Unix), symlink rejection, and atomic rename.
3. `SEC-10`: Configure explicit HTTP/2 stream, window, frame, and TCP keepalive limits on the tonic gRPC server.

## Acceptance Criteria
1. `SecretBytes`, `PkceVerifier`, `ClientSecret`, `Password`, and temporary buffers in `password.rs` zeroize their bytes on drop using `std::hint::black_box`.
2. `write_private_file` in `crates/aegis-client/src/settings.rs` creates sibling temp files with `create_new(true)` and `mode(0o600)` on Unix, rejects symlinks at the target path, and renames atomically.
3. `crates/aegis-server/src/server.rs` sets `http2_max_concurrent_streams(128)`, `initial_stream_window_size(1 MiB)`, `initial_connection_window_size(2 MiB)`, `max_frame_size(64 KiB)`, and `tcp_keepalive(60s)`.
