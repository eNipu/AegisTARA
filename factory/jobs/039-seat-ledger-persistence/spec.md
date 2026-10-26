# Spec — 039-seat-ledger-persistence

## Title
Rebuild concurrent editor seat ledger on server restart from project op history

## Design Reference
- Reference: `docs/adr/0033-license-and-seats.md`
- Affected crates/paths: `crates/aegis-server/src/license.rs`, `crates/aegis-server/src/store/postgres.rs`, `crates/aegis-server/src/main.rs`

## Problem & Goal
Rebuilds the active editor seat table (`SeatTable`) on server startup from recent project operation timestamps (`store.recent_editors`), ensuring a server restart does not reset active seat counts.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0033-license-and-seats.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Prevents bypassing concurrent seat limits by restarting `aegis-server`.
3. UX invariants hold: Consistent seat accounting across rolling server restarts.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
