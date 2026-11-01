# Decision — 114-modeless-create

## Decision
APPROVE

All eleven element types are created from a draft in the inspector with the keyboard alone; nothing is sent until the
element is complete; Escape or an empty name sends nothing; names save without a Rename button; read-only users see
why they cannot add; a refused create stays in the draft with the user's text. The four round-1 reviews PASS; fmt,
clippy `-D warnings`, `cargo test --workspace --locked` (84 binaries, 0 failed) and `scripts/deny.sh` pass. Every
slice was built test first, and the reuse table in the spec held: Slint's text input and IME are reused, only the
draft state is new, no dependency was added.

## Follow-ups for a person (not gates)
- Japanese IME check on macOS (Romaji, Kana) and Windows (Microsoft IME): the Enter that commits a composition must
  not create the element.
- A person's time to add five assets, before and after (37 key presses, no mouse, against 41 inputs with 5 clicks).
- Advisory: draft row not italic grey; File > New project… still prompts for a name.
