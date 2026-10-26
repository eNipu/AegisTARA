# Spec — 029-server-skeleton

## Title
Tonic gRPC service (`aegis-server`, `aegis-proto`) with per-project actor and 2-minute lease lock

## Design Reference
- Reference: `docs/adr/0026-server-rpc-and-actor-core.md`
- Affected crates/paths: `crates/aegis-proto`, `crates/aegis-server`

## Problem & Goal
Adds `crates/aegis-proto` and `crates/aegis-server`: Tonic gRPC service validating and applying `Op`s through `aegis-model` with one actor task per open project (rule C1), 2-minute exclusive edit lease (`LeaseState`, D4), bounded observer streams (rule R4), and decode caps at the API boundary (rule R3).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0026-server-rpc-and-actor-core.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Enforces rule C1 (single task owns mutable `Project`), rule R3 (4 MiB gRPC message cap, 256 KiB op cap, 256 ops/batch cap), rule R4 (bounded mpsc/broadcast queues), and constant-time lease token checks (`subtle::ConstantTimeEq`).
3. UX invariants hold: Structured gRPC error codes (`LOCK_HELD`, `STALE_BASE`, `MALFORMED_OP`, `REJECTED_OP`) with clear human-readable messages.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
