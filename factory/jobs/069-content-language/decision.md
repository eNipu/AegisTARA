# Decision — 069-content-language

## Decision
Descriptions carry optional translations keyed by a BCP 47 language tag, edited
with one invertible op and stored additively so v1 fixtures keep loading.
Bilingual reports read the translation for the non-source language.

## Key choices
- The source text is the description's own text (English); a translation is a
  `(lang, text)` pair, capped at 8 translations and `ElementRef::max_chars`
  text each, with a 16-byte language-tag cap.
- One op, `SetDescriptionTranslation`, covers set and clear (empty text); its
  inverse restores the previous translation, so undo is exact.
- Snapshot `translations` and the op-log record are additive
  (`#[serde(default)]` on the field, a new op variant), keeping v1 fixtures
  byte-for-byte readable.
- The translation editor is inline in the inspector under the description
  (UI-08.3 name/field rules); the modal description editor stays until job 117.
- Reports use the translation when the report language is not the source
  language, so `ReportLanguage::Bilingual` emits both.
