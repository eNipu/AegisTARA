# UX Review — 089-suggestion-branches (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## UX Evaluation
- Clicking `"Suggest steps"` overlays proposed steps directly on the attack-tree canvas as dashed-accent ghost nodes (`ghost · step`) and auto-fits the viewport so all ghost candidates are immediately visible.
- Analysts can click any ghost node on the canvas to highlight it in the `"AI suggestions (Gear 1)"` panel, inspect its confidence, catalog citation, and rationale, and accept or dismiss suggestions individually in any order or all at once, with full `Ctrl+Z` undo support.
