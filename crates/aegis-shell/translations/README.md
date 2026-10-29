# UI translations and translator glossary

The Slint window translates its text with `@tr(...)` in `ui/*.slint` and the
bundled gettext catalogs below:

- `ja/LC_MESSAGES/aegis-shell.po` — Japanese (the shipped translation).
- `pseudo/LC_MESSAGES/aegis-shell.po` — the pseudo-locale for layout tests
  (not shipped to users).

The source language is English, written inline in the `.slint` files; the
default catalog is the source, so a missing `msgid` falls back to English
rather than showing an error. `crates/aegis-shell/tests/i18n.rs` fails the
build when a shipped locale lacks a key.

To add or change a string: edit the `.slint` file, then add the `msgid` to
each `.po` (the English source and its translation). A professional
translator uses the glossary below (job 068, owner task).

## Glossary (English — 日本語)

| English | 日本語 |
|---|---|
| attack potential | 攻撃ポテンシャル |
| attack step | 攻撃ステップ |
| attack tree | 攻撃ツリー |
| asset | 資産 |
| baseline | ベースライン |
| claim | 主張 |
| control (security control) | 対策（セキュリティ対策） |
| damage scenario | 損害シナリオ |
| feasibility | 攻撃可能性 |
| gate | ゲート |
| goal | 目標 |
| impact | 影響 |
| likelihood | 起こりやすさ |
| residual risk | 対策後リスク |
| risk | リスク |
| risk register | リスクレジスタ |
| threat scenario | 脅威シナリオ |
| treatment | 対応（リスク対応） |
| verification | 検証 |
| workflow / methodology | 手法 |
