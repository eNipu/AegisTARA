# Spec — 048-update-manifest-fetch

## Title
HTTPS signed update manifest fetch (`fetch_manifest`) and Help menu update check

## Design Reference
- Reference: `docs/adr/0042-online-update-check.md`
- Affected crates/paths: `crates/aegis-client/src/updates.rs`, `crates/aegis-client/src/session.rs`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Fetches the signed update manifest over HTTPS using `aegis-auth::http::HyperClient` (`fetch_manifest`, `UpdateChecker::check_url`), verifies its Ed25519 signature (`AEGIS-UPDATE-V1`) and SemVer monotonicity, and exposes `Check for updates` in the UI.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0042-online-update-check.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: HTTPS-only (except loopback for tests), no redirects, 10s timeout, 64 KiB manifest cap, and Ed25519 signature verification before parsing.
3. UX invariants hold: On-demand update check with zero unsolicited background network traffic on air-gapped deployments.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
