# Build Report — 110-deny-dev-policy

## Summary
Updated the supply-chain license check (`deny.toml`, `scripts/deny.sh`, and `tools/deny-fixtures/`) for widened Decision D5 so development-only dependencies and the non-shipped fuzz crate are exempt from the shipped-binary license allow-list while still checked for advisories, bans, and sources (`docs/adr/0003-cargo-deny-policy.md`).

## Key Changes
- `deny.toml`: Removed the NCSA exception that was only needed for `libfuzzer-sys` in the fuzz crate; documented dev-dependency behavior.
- `scripts/deny.sh`: Runs `advisories bans licenses sources` on workspace members and `advisories bans sources` on `crates/aegis-model/fuzz`.
- `tools/deny-fixtures/`: Added `ok-dev-gpl` and `bad-gpl-also-dev` fixtures (11 fixtures total in `tools/deny-fixtures/run.sh`).

## Verification
- `./scripts/deny.sh`: PASS
- `./tools/deny-fixtures/run.sh`: PASS (11/11 fixtures)
