# UX Review — 115-row-actions (Round 1)

VERDICT: CHANGES

## Blocking findings
1. **Return does not move focus into the inspector** (UI-09.4). It re-picks the row and focus stays in the table, so "Enter to edit this" does not work from the keyboard. This is a stated acceptance criterion.

## What was checked
- + by row type: component to asset, asset to threat, damage to threat, threat to attack tree, control to goal; the draft opens in the inspector with the parent filled.
- Real keys after a click: Up, Down, End move the selection; Control+D duplicates; Delete removes with an "... removed · Undo" toast; Control+Return starts the child draft. No selected row, no action.
- A read-only user sees no buttons and Delete / Control+D do nothing.
- Names and tooltips: accessible names carry the row name ("Add threat scenario to ECU", "Duplicate ECU"); the name cell elides with a tooltip; four-digit IDs now fit.
- At 5,000 rows scrolling and selection stay responsive on the CPU renderer (9.5 ms per frame).

## Advisory findings
- The draft row also shows + and ⋯, which do nothing useful on it.
- The ⋯ menu is pointer-only; there is no arrow-key path inside it (the shortcuts cover the same actions).
- A duplicated element is named "<name> copy" in English in every language.

## Follow-ups for a person (not gates)
- Feel of scrolling 5,000 rows on a real window with the GPU renderer.
- VoiceOver / NVDA: announcement of the table, the row buttons and the menu.
- Japanese: the new strings are in the ja catalog; read them.
