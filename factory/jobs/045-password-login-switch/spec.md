# Spec — 045-password-login-switch

## Title
Per-deployment admin switch to disable password login when company OIDC login is active

## Design Reference
- Reference: `docs/adr/0039-password-login-switch.md`
- Affected crates/paths: `crates/aegis-server/src/accounts/mod.rs`, `crates/aegis-client`, `crates/aegis-shell`

## Problem & Goal
Allows administrators on a hybrid (`oidc+password`) server to turn password login off or on at runtime (`SetPasswordLogin`), persisting the switch in `aegis_setting`, immediately terminating all open password sessions when turned off, and hiding the password login option in the client.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0039-password-login-switch.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Refuses disabling password login if no OIDC provider is configured (`NoCompanyLogin`) and re-checks the switch under the session lock after Argon2id hashing to prevent TOCTOU session creation.
3. UX invariants hold: Immediate toggle in the Administration screen and automatic login screen adaptation.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
