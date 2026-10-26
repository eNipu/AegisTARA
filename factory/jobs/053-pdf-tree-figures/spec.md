# Spec — 053-pdf-tree-figures

## Title
Vector attack-tree diagrams in PDF reports with overview and tiled detail panels (`aegis_canvas::tiles`)

## Design Reference
- Reference: `docs/adr/0045-pdf-attack-tree-diagrams.md`
- Affected crates/paths: `crates/aegis-canvas/src/tiles.rs`, `crates/aegis-client/src/pdf.rs`

## Problem & Goal
Renders the canvas `DisplayList` as native vector paths inside the PDF report (`tree_figure`, `draw_panel`), using `plan_tiles` to produce a single panel when readable or an overview panel plus up to `max_panels = 16` detail tiles for large trees, verified by a 1,000,000-node stress test.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0045-pdf-attack-tree-diagrams.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Caps detail panels at `TileLimits::default().max_panels = 16` (`MAX_GRID = 32`) so arbitrarily large trees finish in bounded time and page count.
3. UX invariants hold: PDF exports contain crisp, scalable vector attack-tree diagrams alongside the text outline.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
