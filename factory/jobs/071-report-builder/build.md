# Build Report — 071-report-builder

## Summary


## Key Changes
- profile.rs: ReportProfile/ReportSections/ReportFilters/ReportBranding/ReportLanguage/Classification/RevisionEntry/ReportOptions + audit_summary. - settings.rs report_profiles + report_profile(). - session.rs export takes a profile and records the summary. - proto detail field + server validation. - tests: report_builder.rs.

## Verification
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `./scripts/deny.sh`: PASS
