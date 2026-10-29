# Spec — 066-i18n-foundation

## Title
i18n foundation (L10N-01 to L10N-03)

## Design Reference
- Reference: docs/adr/0057-i18n-and-japanese-localization.md
- Affected crates/paths: crates/aegis-client/src/i18n.rs, crates/aegis-client/locales/{en,ja,pseudo}/main.ftl, crates/aegis-shell/build.rs, crates/aegis-shell/translations/{ja,pseudo}/LC_MESSAGES/aegis-shell.po, crates/aegis-shell/ui/*.slint (@tr), crates/aegis-shell/src/launcher.rs, crates/aegis-client/src/settings.rs

## Problem & Goal
All UI text goes through translation: @tr in Slint with bundled catalogs, a Fluent catalog in aegis-client for pages/errors/reports, a UI language setting defaulting to the OS, a missing-key test, and a pseudo-locale for clipped text.

## Acceptance Criteria
1. Every static string in the .slint files uses @tr and is bundled; 2. aegis-client has en/ja/pseudo Fluent catalogs; 3. Settings.language defaults to the OS and selects the bundled catalog; 4. a test fails when a locale lacks a key; 5. a pseudo-locale exists.
