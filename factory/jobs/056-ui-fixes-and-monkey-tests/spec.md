# Spec — 056-ui-fixes-and-monkey-tests

## Title
UI layout fixes across small window sizes, hostile callback monkey testing, and attack-tree library notes

## Design Reference
- Reference: `docs/review-requirements.md`
- Affected crates/paths: `crates/aegis-shell/ui/app.slint`, `crates/aegis-shell/ui/widgets.slint`, `crates/aegis-shell/tests/monkey.rs`, `crates/aegis-shell/tests/small_windows.rs`, `docs/brainstorm-attack-tree-library.md`

## Problem & Goal
Fixes 7 UI layout/truncation issues across small window sizes (980x600 and 1100x760), adds `tests/small_windows.rs` and `tests/monkey.rs` (firing hostile/random inputs and window resizes across all UI callbacks against a live server), and adds design notes in `docs/brainstorm-attack-tree-library.md`.

## Acceptance Criteria
1. Architecture and implementation match `docs/review-requirements.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Proves via randomized monkey testing (`tests/monkey.rs`) that hostile UI callback arguments (10,000-char strings, NaN/Inf floats, negative/huge indices, malformed outlines) never panic or corrupt client/server state.
3. UX invariants hold: Responsive panel widths and clean table column sizing down to 980x600 without clipping controls or labels.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
