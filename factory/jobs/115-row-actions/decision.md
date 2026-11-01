# Decision — 115-row-actions

## Decision
APPROVE

The whole UI-09 scope is done: `DataTable` with the old API (the `StandardTableView` wrapper is gone), + and ⋯ with accessible names, + by row type, Open / Duplicate / Remove, arrow keys, Return into the inspector, Control+Return, Delete and Control+D with real key events, sticky header, truncation with tooltip, read-only suppression, and a measured 5,000-row run. The four round-1 reviews PASS. fmt, clippy `-D warnings`, `cargo test --workspace --locked` (1,632, 0 failed) and `scripts/deny.sh` pass. No dependency was added; every new string is in the ja and pseudo catalogs.

The first pass of this round escalated on three gaps (Return did not focus the inspector, the draft row showed actions, the Duplicate suffix was English); each is fixed and tested. The first screenshot of the round also found a slice 1 layout defect (header below the rows), fixed.

## Follow-ups for a person (not gates)
- Scroll and selection feel at 5,000 rows on a real window with the GPU renderer. Measured on the software renderer, headless: scroll frame 9.5 ms median, select-next 9.2 ms, End 10.3 ms.
- VoiceOver / NVDA on the table, the row buttons and the ⋯ menu; a read of the ja strings.
- Advisory: the ⋯ menu has no arrow-key path (the shortcuts cover it); no per-button focus ring inside a row; the draft row is not italic grey.
