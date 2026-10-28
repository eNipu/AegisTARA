# Spec — 110-deny-dev-policy

## Title
License check policy for development-only tools (`D5`)

## Design Reference
- Reference: `docs/adr/0003-cargo-deny-policy.md`, `docs/decisions.md` (D5)
- Affected crates/paths: `deny.toml`, `scripts/deny.sh`, `tools/deny-fixtures/run.sh`, `tools/deny-fixtures/ok-dev-gpl/`, `tools/deny-fixtures/bad-gpl-also-dev/`

## Problem & Goal
Per the widened Decision D5 (2026-10-26), tools used only during development (`[dev-dependencies]` and the non-shipped `crates/aegis-model/fuzz` target) may use any license as long as nothing from them is linked into shipped binaries. Verify and pin `cargo-deny` behavior with regression fixtures and update `scripts/deny.sh`.

## Acceptance Criteria
1. `deny.toml` continues to enforce the permissive allow-list on all shipped dependencies while excluding `[dev-dependencies]` from the license check.
2. `scripts/deny.sh` runs `advisories bans licenses sources` on the workspace and `advisories bans sources` on the non-shipped `crates/aegis-model/fuzz` crate (removing the special NCSA exception for `libfuzzer-sys`).
3. Two new fixtures (`ok-dev-gpl` and `bad-gpl-also-dev`, 11 fixtures total) verify that a GPL dev-only dependency passes while a GPL dependency that is also a normal dependency fails.
4. `./scripts/deny.sh` and `./tools/deny-fixtures/run.sh` pass.
