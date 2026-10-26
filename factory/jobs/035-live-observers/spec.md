# Spec — 035-live-observers

## Title
Live read-only observers streaming accepted ops and recomputed risk results

## Design Reference
- Reference: `docs/adr/0030-client-core.md`
- Affected crates/paths: `crates/aegis-server/src/actor.rs`, `crates/aegis-client/src/session.rs`

## Problem & Goal
Streams accepted operations, lock-holder transitions, and recomputed risk results from the server's project actor to all subscribed read-only clients (`Watch` RPC), with automatic resync when an observer falls behind the in-memory backlog ring.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0030-client-core.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Bounded broadcast channel (`observer_buffer`) and bounded backlog ring (`recent_ops`); lagging observers receive `OUT_OF_RANGE` and cleanly reload the latest snapshot.
3. UX invariants hold: Read-only observers see the active editor's changes and lock holder in real time without manual refresh.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
