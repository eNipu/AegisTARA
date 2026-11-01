# UI Review — 115-row-actions (Round 1)

VERDICT: PASS

## Blocking findings
None. (A blocking defect, the header drawn below the rows, was found by the first screenshot of this round and fixed before this review.)

## Advisory findings
- The ⋯ button shows three dots as "..." at this size; it reads, but is lower than the + glyph.
- Hover and selection use the OS accent and selection colours; the hover row on the selected-accent blue is a lighter blue that is close to the selected row's colour on macOS. Check Dark mode.
- A draft row is not italic grey (a table cell cannot style one row; still open from job 114). It now shows no + or ⋯.

## What is good
- Header bar above the rows, 28 px; rows 32 px (28 compact); alternate rows and the accent selected row; long names elide.
- The + and ⋯ buttons sit at the right end of the row only on hover or selection, so a plain table stays quiet.
- Screenshots viewed: `datatable-debug.png` (two rows, header on top, buttons on the selected row) and `datatable-5000-scrolled.png` (rows around 3,150 with the scroll bar and IDs not elided).
