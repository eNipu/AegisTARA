# Spec — 027-workspace-and-readme

## Title
Root Cargo workspace, shared dependencies/lints, CI scripts, and README refresh

## Design Reference
- Reference: `docs/adr/0025-cargo-workspace.md`
- Affected crates/paths: `Cargo.toml`, `README.md`, `scripts/ci.sh`, `scripts/deny.sh`

## Problem & Goal
Promotes the repository root to a Cargo workspace (`members = ["crates/*"]`, shared `[workspace.dependencies]` and `[workspace.lints]`), moves `Cargo.lock` to the root, adapts `scripts/ci.sh`, `scripts/deny.sh`, and CI workflows, and refreshes `README.md` and `docs/direction-v1.md` (gaps G1, G2, G6).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0025-cargo-workspace.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Workspace-wide `unsafe_code = "forbid"`, `unwrap_used = "deny"`, `expect_used = "deny"`, and `panic = "deny"` lints plus unified `Cargo.lock` auditing via `scripts/deny.sh`.
3. UX invariants hold: Single root-level `cargo test` / `scripts/ci.sh` workflow and accurate documentation.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
