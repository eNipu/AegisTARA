# Build Report — 070-report-doc-model

## Summary


## Key Changes
- report/doc.rs: ReportDoc/Block/Style/Figure/Panel + report_doc_with_options (the old report_html and pdf::report_doc merged). - report/html.rs renders the doc with nav.toc and sec-N anchors. - report/pdf.rs keeps layout/render only.

## Verification
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `./scripts/deny.sh`: PASS
