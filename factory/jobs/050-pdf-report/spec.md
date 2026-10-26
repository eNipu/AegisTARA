# Spec — 050-pdf-report

## Title
Direct deterministic A4 PDF report export (`crates/aegis-client/src/pdf.rs`) with `krilla` and Inter font

## Design Reference
- Reference: `docs/adr/0044-direct-pdf-report.md`
- Affected crates/paths: `crates/aegis-client/src/pdf.rs`, `crates/aegis-client/assets/fonts`, `deny.toml`

## Problem & Goal
Adds native, deterministic A4 PDF report generation (`report_pdf`) using `krilla` and `skrifa` with embedded `Inter-Regular.otf` / `Inter-Bold.otf` fonts (OFL-1.1), repeating table headers across page breaks, page numbers, and confidentiality watermarks on every page.

## Acceptance Criteria
1. Architecture and implementation match `docs/adr/0044-direct-pdf-report.md` and `docs/engineering-standards.md` (rules P1, C1, R3, R4, R5, R6, Q3, Q4, Q7).
2. Security invariants hold: Uses `skrifa` glyph metrics directly (avoiding `krilla`'s unmaintained `rustybuzz`/`ttf-parser` feature, `RUSTSEC-2026-0206`) and caps output at `MAX_PAGES = 2000` (R6).
3. UX invariants hold: Direct `Export PDF` produces an offline, print-ready TARA work product identical in structure to the HTML report.
4. All unit, integration, and property tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
