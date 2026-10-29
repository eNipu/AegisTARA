# Spec — 068-japanese-translation

## Title
Japanese translation (L10N-06, L10N-07)

## Design Reference
- Reference: docs/adr/0057-i18n-and-japanese-localization.md
- Affected crates/paths: crates/aegis-shell/translations/ja/LC_MESSAGES/aegis-shell.po, crates/aegis-client/locales/ja/main.ftl, crates/aegis-client/src/report/doc.rs (language-aware builder)

## Problem & Goal
Full Japanese translation of the UI and reports. Per-language labels in methodology packs (format change) and Japanese labels for the built-in pack are the remaining piece of L10N-07 and land with the pack format change.

## Acceptance Criteria
1. ja catalogs for UI and reports; 2. reports render Japanese headings/dates; 3. per-language pack labels (L10N-07) remain open and are noted in the ADR and architecture.
