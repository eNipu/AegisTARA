# Spec — 068-japanese-translation

## Title
Japanese translation (L10N-06, L10N-07)

## Design Reference
- Reference: docs/adr/0057-i18n-and-japanese-localization.md
- Affected crates/paths: crates/aegis-shell/translations/ja/LC_MESSAGES/aegis-shell.po, crates/aegis-client/locales/ja/main.ftl, crates/aegis-client/src/report/doc.rs (language-aware builder)

## Problem & Goal
Full Japanese translation of the UI and reports, plus per-language labels in methodology packs (backward-compatible format extension) and Japanese labels for the built-in `iso21434-r155` pack wired to the language-aware report builder (`L10N-06`, `L10N-07`).

## Acceptance Criteria
1. ja catalogs for UI and reports; 2. reports render Japanese headings/dates; 3. per-language pack labels (`L10N-07`) in `Methodology` and `iso21434-r155.json` render localized element-type headings and vocabulary labels in Japanese and Bilingual reports while older packs load unchanged.
