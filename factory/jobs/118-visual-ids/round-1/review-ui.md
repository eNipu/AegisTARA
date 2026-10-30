# UI Review — 118-visual-ids (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## UI Evaluation
- Element tables in the desktop shell get a leading `ID` column; the headless
  UI tests assert the first cell of the asset, threat and attack-tree rows is
  the visual ID (`A00`, `T00`, `AT00`).
- Detail/inspector titles and link lists render the ID with a ` · ` separator
  (`"A00 · Brake request on CAN-2"`), matching the existing link-list
  convention in `controller.rs`.
- Reports and exports (HTML/doc, CSV, XLSX) surface the ID as a leading column
  or an `"ID · name"` cell, consistent across tables.
- The command palette row still leads with `"ID · name"`, and the palette
  headless test checks both navigation and the rendered label prefix.
- `crates/aegis-shell/ui/app.slint` adds the new `ID` columns and inspector
  fields; Japanese and pseudo-locale translation keys were updated, and the
  i18n test (`every_shipped_locale_has_every_key`) passes.
