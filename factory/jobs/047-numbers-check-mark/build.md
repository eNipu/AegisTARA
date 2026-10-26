# Build Report — 047-numbers-check-mark

## Summary
Compares the client engine's risk numbers against the server's `RisksEvent` on both observer updates and confirmed editor saves (`compare_server_risks`), surfacing `NumbersCheck` (`Verified`, `Mismatch`, `VersionMismatch`, `Pending`) in the header and falling back to server numbers if a mismatch ever occurs (gap G15).

## Key Changes
- Design documented in `docs/adr/0043-verify-client-risks.md`.
- Implemented across `crates/aegis-server/src/actor.rs`, `crates/aegis-client/src/session.rs`, `crates/aegis-shell/ui/app.slint`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
