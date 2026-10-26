# Spec — 034-license-tokens

## Title
Offline Ed25519 license tokens (`aegis-license`) and server license enforcement

## Design Reference
- Reference: `docs/adr/0033-license-and-seats.md`
- Affected crates/paths: `crates/aegis-license`, `crates/aegis-server/src/license.rs`

## Problem & Goal
Creates `crates/aegis-license` to sign and verify offline Ed25519 license files (`AEGIS-LICENSE-V1`: customer, edition, seats, expiry, features) with zero network calls; customer builds of `aegis-server` require a valid license at startup and degrade to read-only mode on expiry.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0033-license-and-seats.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Domain-separated Ed25519 signatures via `ring` (`AEGIS-LICENSE-V1`), 64 KiB size cap, and fail-safe read-only degradation on expiry.
3. UX invariants hold: Clear license status and read-only notifications so customers never lose read/export access to existing safety analyses.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
