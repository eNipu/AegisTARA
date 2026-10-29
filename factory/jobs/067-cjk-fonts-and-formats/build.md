# Build Report — 067-cjk-fonts-and-formats

## Summary


## Key Changes
- NotoSansJP-Variable.ttf + LICENSE-notosansjp.txt. - report/pdf.rs: three faces, Fonts::glyph/width fallback, per-font run segmentation in draw_text. - time.rs format_ms_lang. - tests: pdf.rs the_cjk_fallback_maps_japanese_without_a_question_mark; report_builder japanese_text_survives_in_the_pdf.

## Verification
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `./scripts/deny.sh`: PASS
