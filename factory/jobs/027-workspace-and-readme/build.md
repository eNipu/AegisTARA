# Build Report — 027-workspace-and-readme

## Summary
Promotes the repository root to a Cargo workspace (`members = ["crates/*"]`, shared `[workspace.dependencies]` and `[workspace.lints]`), moves `Cargo.lock` to the root, adapts `scripts/ci.sh`, `scripts/deny.sh`, and CI workflows, and refreshes `README.md` and `docs/direction-v1.md` (gaps G1, G2, G6).

## Key Changes
- Design documented in `docs/adr/0025-cargo-workspace.md`.
- Implemented across `Cargo.toml`, `README.md`, `scripts/ci.sh`, `scripts/deny.sh`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
