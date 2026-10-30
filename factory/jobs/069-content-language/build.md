# Build — 069-content-language

## What was built
- Model: `Translation` and the `SetDescriptionTranslation` op with its inverse;
  `Project::description_translation` / `description_translations`;
  `MAX_DESCRIPTION_TRANSLATIONS = 8` and `MAX_LANG_TAG_BYTES = 16`; validation
  (existing description, capped count, capped text, BCP 47 tag) and the
  `description_bytes` budget.
- Store: snapshot `translations` on descriptions (`#[serde(default)]`, so v1
  fixtures load) and the `set_description_translation` op-log record; both are
  additive, so v1 fixtures stay byte-for-byte readable.
- Client: `Edit::SetDescriptionTranslation`, the undo test in
  `tests/descriptions.rs` that applies and undoes the op and checks the project
  is restored exactly, and `DescriptionView::translations`.
- Report: `description_of`/`section_descriptions` take the report language and
  use the translation when one exists, so bilingual HTML/PDF/XLSX reports
  include translated descriptions.
- Shell: an inline `TranslationEditor` under every description in the
  inspector (language tag + text + save/remove, no new modal); the existing
  modal description editor is unchanged and removed later by job 117.

## Verified
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.
- `cargo test -p aegis-model` (610 tests, incl. snapshot/op-log round trips).
- `cargo test -p aegis-client --test descriptions --lib`.
- `cargo test --workspace` (ran unsandboxed; loopback bind and OS trust store are blocked by the sandbox).
- `./scripts/deny.sh`.

## Not built (remaining)
- None.
