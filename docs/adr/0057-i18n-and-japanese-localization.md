# ADR 0057: i18n foundation and Japanese localization

- Status: accepted (jobs 066-i18n-foundation, 067-cjk-fonts-and-formats,
  068-japanese-translation)
- Date: 2026-10-26
- Scope: `crates/aegis-client/src/i18n.rs`,
  `crates/aegis-client/locales/{en,ja,pseudo}/main.ftl`,
  `crates/aegis-client/src/report/` (language-aware builder),
  `crates/aegis-shell/build.rs`, `crates/aegis-shell/translations/`,
  `crates/aegis-shell/ui/*.slint` (`@tr`),
  `crates/aegis-client/assets/fonts/NotoSansJP-Variable.ttf`
- Rules: D5 (licenses), P4 (determinism), L10N-01 to L10N-07
- Builds on: `0044-pdf-report.md` (font embedding), `0056-...` (report model)

## Context

The UI text was hard-coded (about 226 `text:` lines), the PDF font (Inter)
had no Japanese glyphs, dates were English only, and there was no language
setting. Japanese customers need a Japanese UI and Japanese reports.

## Decisions

### 1. Two translation layers, each where it belongs

Slint translates its own text with `@tr(...)` and bundled gettext catalogs
(`crates/aegis-shell/translations/{ja,pseudo}/LC_MESSAGES/aegis-shell.po`),
compiled into the binary by `slint_build` with
`DefaultTranslationContext::None` so a `.po` is a plain msgid/msgstr map, and
selected at run time with `slint::select_bundled_translation` after the first
component exists. The client core translates what it builds (pages, errors,
reports) with Fluent (`fluent-bundle`) catalogs in
`crates/aegis-client/locales/{en,ja,pseudo}/main.ftl`, bundled with
`include_str!`. The two never overlap: Slint owns window text, Fluent owns
report text.

### 2. The pseudo-locale is a first-class locale

Both layers ship a pseudo-locale that transforms the English source (visible
markers, accented glyphs, longer strings) so clipped or hard-coded text shows
up in screenshots. It is hidden from shipped users but available to tests. The
client `Language::Pseudo` and the shell `pseudo` catalog both exist, and a
test fails if a shipped locale lacks any `@tr` key.

### 3. The OS picks the default, a setting overrides it

`Language::from_os()` reads `AEGIS_LANGUAGE` (tests, air-gapped sites) and
then `LC_ALL`/`LC_MESSAGES`/`LANG`; `Settings.language` overrides it. The
report language is a separate profile choice (ADR 0056), so the UI and a
report can differ.

### 4. Japanese in the PDF without a shaper

`NotoSansJP-Variable.ttf` (OFL-1.1, `assets/fonts/` with its license) is the
fallback face for characters Inter does not carry. The layout still measures
glyph by glyph with `skrifa`; a mixed Latin+Japanese run is split into
per-font segments before drawing, so both scripts share one line. Only the
used glyphs are embedded. No text shaper is added (RUSTSEC-2026-0206, as in
ADR 0044). Japanese needs no shaping beyond the cmap.

### 5. Locale dates, not a date crate

`time::format_ms_lang` writes Japanese dates as `2026年10月25日 19:30 UTC`
and every other language as the existing English form; the UTC zone stays
named so two readers agree (P4). Numbers are integers in reports, so no
decimal-separator work was needed beyond what Slint's bundled translations
already carry.

### 6. Report language comes from the Fluent catalog

`report/doc.rs` builds the report once per selected language (English,
Japanese, or both side by side for `ReportLanguage::Bilingual`); the PDF
metadata carries the language tag. Missing catalog keys render as `[id]` so a
gap is visible, and Fluent's bidi-isolation marks (U+2068/U+2069) are stripped
from the final text so reports and captions compare byte-for-byte.

## Consequences

- The desktop binary grows by the Inter fonts plus Noto Sans JP (~9 MiB);
  only the used glyphs are embedded in each PDF.
- A test (`crates/aegis-shell/tests/i18n.rs`) fails when a shipped locale
  lacks a key, and the pseudo-locale catalogs let a screenshot test catch
  clipped text.
- The built-in methodology pack labels per language (L10N-07) are the
  remaining piece of job 068 and land with the pack format change; the report
  element-type headings still read from the pack's single set of labels today.

## Alternatives

- Slint's `gettext` feature (`.mo` files loaded at run time) was rejected:
  it needs a system gettext, the catalogs would not be bundled in the binary,
  and it is a different path from the bundled catalogs used here.
- A full text shaper for the PDF was rejected (unmaintained dependency);
  Japanese kana/kanji map one-to-one through the cmap.
