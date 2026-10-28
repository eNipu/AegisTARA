# Build Report — 057-attack-tree-library

## Summary
Implemented the Attack Tree Library Phase 0 (`LibraryPack`, `library-demo.json`, `LibraryIndex` inverted n-gram search, `Op::SetReviewMark` + `LibraryRatingNotReviewed` lint warning, and the Trees-page Library panel in `aegis-shell`), documented in ADR 0052 (`docs/adr/0052-attack-tree-library.md`).

## Key Changes
- `crates/aegis-model/src/library.rs` & `packs/library-demo.json`: `aegis-library` v1 JSON format with SHA-256 content hash verification, `rehash`, built-in 12-item `CC0-1.0` pack from UN R155 Annex 5 and CAPEC with Japanese aliases, and inverted n-gram `LibraryIndex`.
- `crates/aegis-model/src/{op,project,lint}.rs`: Added `Op::SetReviewMark`, automatic clearing when a step or its factors are rated, and `LintCode::LibraryRatingNotReviewed`.
- `crates/aegis-client/src/library.rs`: `LibraryCatalog` loading built-in and folder packs (`~/.config/aegistara/libraries/`), `insert_ops` for `NewTree`, `UnderGate`, and `ReplaceStep`, and origin provenance (`<user>@lib:<pack>@<ver>:<item>`).
- `crates/aegis-shell/ui/app.slint` & `src/controller.rs`: Added the Library panel on the Attack Trees page with search box, facet chips, preview, `Include starting ratings` toggle, insertion buttons, and `Accept rating` in the Inspector.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
