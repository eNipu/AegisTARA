# UX Review — 058-project-access (Round 2, UI)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## UX Evaluation
- Adding a member and changing a role happen in place with no modal; the
  table is the context.
- Removing a member is the one confirmation that stays, because membership is
  not on the project undo history. Per UI-12 it names the thing in the title
  (`Remove Hana Suzuki from 'ECU Gen3'?`) and the main button says the action
  (`Remove`), never "Yes"; Escape cancels.
- The last-owner refusal is inline next to the row (UI-10.11), not a dialog.
