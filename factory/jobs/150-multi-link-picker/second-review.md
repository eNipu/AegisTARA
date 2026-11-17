VERDICT: CHANGES (fixed on review/post-129-fixes, 6904153)
Reviewed: 8157742..a44be6b against spec.md and UI-15.

Fine: the filter with IME composition, the bounded window, the clamped cursor, and the typed edits.

Fixed:
1. The picker took a snapshot when it opened and had no way to re-read the project. After a pick was applied, the option still showed as unlinked, so the next key press linked again instead of unlinking. Every test reopened the picker after each edit, which hid this. `refresh(project)` now re-reads the links and keeps the filter and cursor, with a test.
