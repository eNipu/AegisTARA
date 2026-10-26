# Build Report — 050-pdf-report

## Summary
Adds native, deterministic A4 PDF report generation (`report_pdf`) using `krilla` and `skrifa` with embedded `Inter-Regular.otf` / `Inter-Bold.otf` fonts (OFL-1.1), repeating table headers across page breaks, page numbers, and confidentiality watermarks on every page.

## Key Changes
- Design documented in `docs/adr/0044-direct-pdf-report.md`.
- Implemented across `crates/aegis-client/src/pdf.rs`, `crates/aegis-client/assets/fonts`, `deny.toml`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
