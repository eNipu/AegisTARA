# Spec — 067-cjk-fonts-and-formats

## Title
CJK fonts and formats (L10N-04, L10N-05)

## Design Reference
- Reference: docs/adr/0057-i18n-and-japanese-localization.md
- Affected crates/paths: crates/aegis-client/assets/fonts/NotoSansJP-Variable.ttf (+license), crates/aegis-client/src/report/pdf.rs, crates/aegis-client/src/time.rs

## Problem & Goal
Japanese renders in the PDF (embedded Noto Sans JP fallback) and dates/numbers follow the locale.

## Acceptance Criteria
1. Noto Sans JP (OFL-1.1) is embedded as the PDF fallback for characters Inter lacks; 2. mixed Latin+Japanese runs are split per font; 3. format_ms_lang writes Japanese dates; 4. a test proves Japanese glyphs come from the CJK face, not ?.
