# Spec — 033-client-core-and-slint-shell

## Title
Client core (`aegis-client`) with optimistic edits over `ProjectEvalCache` and Slint desktop shell (`aegis-shell`)

## Design Reference
- Reference: `docs/adr/0030-client-core.md`
- Affected crates/paths: `crates/aegis-client`, `crates/aegis-shell`, `deny.toml`

## Problem & Goal
Builds `crates/aegis-client` (single-actor `Session`, `Workspace` with optimistic edits and inverse-op undo/redo over `ProjectEvalCache`, lease renewal, and `ViewModel` snapshots) and `crates/aegis-shell` (Slint desktop UI, headless-tested on the software renderer, D7).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0030-client-core.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Confines Slint strictly to `aegis-shell` in `deny.toml`; client actor never blocks on network calls and drops stale epoch events.
3. UX invariants hold: Sub-5 ms optimistic editing, automatic lease renewal, and full keyboard/mouse workspace navigation.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
