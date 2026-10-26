# Build Report — 034-license-tokens

## Summary
Creates `crates/aegis-license` to sign and verify offline Ed25519 license files (`AEGIS-LICENSE-V1`: customer, edition, seats, expiry, features) with zero network calls; customer builds of `aegis-server` require a valid license at startup and degrade to read-only mode on expiry.

## Key Changes
- Design documented in `docs/adr/0033-license-and-seats.md`.
- Implemented across `crates/aegis-license`, `crates/aegis-server/src/license.rs`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
