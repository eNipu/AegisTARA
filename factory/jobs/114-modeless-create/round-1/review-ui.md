# UI Review — 114-modeless-create (Round 1)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- The draft row is plain text on the selected blue row, not italic grey as UI-08 describes; a table cell cannot style
  a single row. A `DataTable` (job 115) could.
- Draft forms put the name first and the required choices below it; the Create button is full width and disabled
  with "Choose the required fields" until complete. Check the disabled button contrast in the dark mode.
- On the trees page the draft form sits under the tree list, not in an inspector (the page has none).

## What is good
- One `DraftForm` for every element, one look: title, field with focus, hint, required choices, Create.
- The field error (icon + error colour) is shared with the login and password screens.
- Screenshots viewed: `draft-asset.png`, `draft-damage-next.png`, `draft-threat-incomplete.png`,
  `draft-item-definition.png`, `name-editor-empty.png`.
