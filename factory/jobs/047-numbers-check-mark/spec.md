# Spec — 047-numbers-check-mark

## Title
Continuous dual-engine risk verification badge (`NumbersCheck`) across editor and observer flows

## Design Reference
- Reference: `docs/adr/0043-verify-client-risks.md`
- Affected crates/paths: `crates/aegis-server/src/actor.rs`, `crates/aegis-client/src/session.rs`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Compares the client engine's risk numbers against the server's `RisksEvent` on both observer updates and confirmed editor saves (`compare_server_risks`), surfacing `NumbersCheck` (`Verified`, `Mismatch`, `VersionMismatch`, `Pending`) in the header and falling back to server numbers if a mismatch ever occurs (gap G15).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0043-verify-client-risks.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Guarantees safety integrity between client and server engines: any discrepancy triggers an immediate reload and displays authoritative server risk ratings.
3. UX invariants hold: Gives safety engineers explicit visual confirmation that local risk calculations match the server at the current sequence number.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
