# Spec — 032-canvas-display-list

## Title
Headless attack-tree layout, spatial hit testing and culling, display list, and SVG renderer (`aegis-canvas`)

## Design Reference
- Reference: `docs/adr/0029-canvas-core.md`
- Affected crates/paths: `crates/aegis-canvas`

## Problem & Goal
Builds `crates/aegis-canvas`: linear-time layered DAG layout (`layout_tree`, rule A6), viewport culling and reverse-order hit testing (rule A5), toolkit-agnostic `DisplayList`, Okabe-Ito color-blind-safe `Palette`, and deterministic `scene_to_svg` export.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0029-canvas-core.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Pure safe Rust (`#[forbid(unsafe_code)]`), strict XML escaping on all SVG text/attributes, finite geometry checks, and bounded node counts.
3. UX invariants hold: Every node renders both color tone and explicit text badge + gate prefix (`OR`/`AND`/`SEQ`) for color-blind accessibility.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
