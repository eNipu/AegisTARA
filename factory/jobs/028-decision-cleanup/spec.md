# Spec — 028-decision-cleanup

## Title
Resolve dangling worktrees/branches and record D4 & D7 outcomes in docs/decisions.md

## Design Reference
- Reference: `docs/decisions.md`
- Affected crates/paths: `docs/decisions.md`, `docs/status-and-roadmap.md`

## Problem & Goal
Cleans up dangling git worktrees and branches and records the architectural decisions for D4 (2-minute project edit lease lock) and D7 (Slint desktop UI toolkit confined to `aegis-shell`) in `docs/decisions.md` (gaps G3–G5).

## Acceptance Criteria
1. Architecture and implementation match `docs/decisions.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Documents the explicit `cargo-deny` confinement rule for Slint (`wrappers = ["aegis-shell"]`) and server-enforced lease tokens.
3. UX invariants hold: Clear decision log and roadmap traceability before phase-2 multi-crate implementation.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
