# Spec — 046-image-ci

## Title
CI workflow building the server container image and verifying license gate, `/healthz`, and `GetLoginOptions`

## Design Reference
- Reference: `docs/adr/0041-health-endpoint.md`
- Affected crates/paths: `.github/workflows/ci.yml`, `Containerfile`

## Problem & Goal
Builds the production `Containerfile` in CI and verifies both runtime invariants: customer builds refuse to start without a valid license file, and a configured test container starts cleanly and answers `GET /healthz` and `GetLoginOptions` (gap G12).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0041-health-endpoint.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Proves in CI that the license gate cannot be accidentally compiled out of customer container images.
3. UX invariants hold: Automated container smoke testing on every push/PR.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
