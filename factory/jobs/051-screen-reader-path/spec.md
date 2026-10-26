# Spec — 051-screen-reader-path

## Title
Screen-reader accessibility descriptions and keyboard navigation for attack-tree canvas nodes

## Design Reference
- Reference: `docs/adr/0046-attack-tree-editing.md`
- Affected crates/paths: `crates/aegis-client/src/view.rs`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Provides a complete screen-reader sentence for the selected attack-tree node (`canvas_a11y_label`: node label, gate/step kind, inherent/residual feasibility, and child count) and full keyboard reach and activation on the canvas (gap G14).

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0046-attack-tree-editing.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Pure view-model formatting over validated model state.
3. UX invariants hold: Screen-reader and keyboard-only users can navigate and inspect attack-tree nodes without relying on visual canvas coordinates.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
