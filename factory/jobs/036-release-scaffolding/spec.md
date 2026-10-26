# Spec — 036-release-scaffolding

## Title
Release build profile, server Containerfile, Docker Compose, and signed update manifest verification

## Design Reference
- Reference: `docs/adr/0034-signed-update-manifest.md`
- Affected crates/paths: `Cargo.toml`, `Containerfile`, `deploy/compose.test.yaml`, `crates/aegis-license/src/manifest.rs`, `crates/aegis-client/src/updates.rs`

## Problem & Goal
Adds release profile (`lto = "thin"`, `strip = "symbols"`, `codegen-units = 1`), multi-stage `Containerfile` (non-root UID 10001 `aegis`), `deploy/compose.test.yaml`, and Ed25519-signed update manifest verification (`AEGIS-UPDATE-V1`) in `aegis-license` and `aegis-client`.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0034-signed-update-manifest.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Separate Ed25519 trust roots for updates vs. licenses, strict SemVer anti-downgrade check, and non-root container execution.
3. UX invariants hold: Reproducible release builds and one-command local Docker Compose test stack.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
