# Decision — 115-row-actions

## Decision
ESCALATE

Four slices are built and the gate passes, but the whole UI-09 scope is not done, and one review (UX) says CHANGES. Code, security and UI PASS.

## What remains
1. **UI-09.4: Return must move focus into the inspector.** It only re-picks the row. Needs a "focus the name field when the inspector appears" mechanism on eleven pages. (UX blocking.)
2. The draft row should not show + and ⋯.
3. The " copy" suffix on Duplicate is English and in the client core; it needs a translatable name rule.
4. A person to confirm the 5,000-row feel with the GPU renderer (the measured 9.5 ms median scroll frame, 10.3 ms for End, is the software renderer, headless).

## What is done
UI-09.1 (`DataTable`, same API, old `Table` removed), UI-09.2 (+ and ⋯ with accessible names), UI-09.3 (+ by row type), UI-09.4 except Return-to-inspector (real key events now tested), UI-09.5 (rows, sticky header, tooltip), read-only suppression, a measured 5,000-row run, no new dependency, strings in ja and pseudo.

## Verified
fmt, clippy `-D warnings`, `cargo test --workspace --locked` (1,630, 0 failed) and `scripts/deny.sh`. Two small edits after that run were rechecked with fmt, clippy on `aegis-shell` and the `aegis-shell` suite.

## Follow-ups for a person (not gates)
Scroll feel on a real GPU window; VoiceOver / NVDA on the table and menu; read the ja strings.
