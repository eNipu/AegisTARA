# Build Report — 048-update-manifest-fetch

## Summary
Fetches the signed update manifest over HTTPS using `aegis-auth::http::HyperClient` (`fetch_manifest`, `UpdateChecker::check_url`), verifies its Ed25519 signature (`AEGIS-UPDATE-V1`) and SemVer monotonicity, and exposes `Check for updates` in the UI.

## Key Changes
- Design documented in `docs/adr/0042-online-update-check.md`.
- Implemented across `crates/aegis-client/src/updates.rs`, `crates/aegis-client/src/session.rs`, `crates/aegis-shell/ui/app.slint`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
