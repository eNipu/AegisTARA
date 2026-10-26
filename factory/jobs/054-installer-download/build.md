# Build Report — 054-installer-download

## Summary
Streams the update installer over HTTPS (`HttpDownload::download`, `UpdateOffer::download`) into an exclusive `<name>.<unique>.part` file (`create_new(true)`, `0700` private dir in temp), enforcing the signed size cap and SHA-256 checksum on the fly before `sync_all()` and atomic rename.

## Key Changes
- Design documented in `docs/adr/0048-verified-update-download.md`.
- Implemented across `crates/aegis-auth/src/http.rs`, `crates/aegis-client/src/updates.rs`, `crates/aegis-client/src/session.rs`, `crates/aegis-shell/ui/app.slint`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
