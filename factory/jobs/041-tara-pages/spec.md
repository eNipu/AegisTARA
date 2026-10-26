# Spec — 041-tara-pages

## Title
Complete TARA pages, outline editor, baselines & diff, history, and HTML/CSV/SVG exports

## Design Reference
- Reference: `docs/adr/0037-baselines-and-reports.md`
- Affected crates/paths: `crates/aegis-client/src/pages.rs`, `crates/aegis-client/src/outline.rs`, `crates/aegis-client/src/report.rs`, `crates/aegis-shell/ui/app.slint`

## Problem & Goal
Implements every TARA workspace page in `aegis-client` and `aegis-shell`: Overview, Assets, Damage scenarios, Threat scenarios, Attack trees (with indented text outline editor), Risks & treatment, Controls, Goals & claims, Problems, Baselines (named snapshots & diff), History, and HTML/CSV/SVG exports.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0037-baselines-and-reports.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Escapes all HTML content (`esc()`), guards CSV cells against formula injection (`csv_field()`), sanitizes export filenames (`export_file_name()`), and records every export on the server.
3. UX invariants hold: End-to-end ISO/SAE 21434 authoring and review workflow with one-click issue navigation and baseline comparison.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
