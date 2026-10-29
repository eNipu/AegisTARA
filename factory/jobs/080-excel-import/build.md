# Build Report — 080-excel-import

## Summary
Implemented `.xlsx` TARA workbook import with worksheet selection, column mapping, dry-run preview, unmapped row reporting, and single-batch apply with `ProvenanceSource::Import` (`INT-02`, ADR 0060).

## Key Changes
- `crates/aegis-client/src/excel.rs`: Bounded ZIP + Deflate + XML `.xlsx` workbook parser, bilingual English/Japanese header auto-detection, dry-run simulation against a cloned `Project`, unmapped row diagnostics, and `Provenance::import` batch generation.
- `crates/aegis-client/src/{commands,session,state,view}.rs` & `crates/aegis-shell/{ui/app.slint,src/controller.rs}`: Excel import workflow on the Reports & Export page with sheet picker, column mapping selectors, dry-run summary, unmapped rows list, and one-click batch import.

## Verification
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `./scripts/deny.sh`: PASS
