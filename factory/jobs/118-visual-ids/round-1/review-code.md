# Code Review — 118-visual-ids (Round 1)

## Verdict
PASS

## Blocking findings
None.

## Advisory findings
None.

## Standards Check
- The prefix scheme is centralised in `VisualPrefix` (`as_str`, `format`,
  `parse_num`); `format` zero-pads to at least 2 digits and grows past `99`,
  and `parse_num` is the single canonical decoder (prefix + 2..10 ASCII
  digits, no leading zeros on 3+ digits), so a malformed ID is refused rather
  than guessed.
- Assignment lives in one place: `Arena::push` (monotonic, `saturating_add`,
  never reuses a deleted number) and `Arena::remove` (remembers the number for
  the `inverse` of `Op::Remove*`). Undo is exercised by the model store tests,
  and `Project::eq` compares live elements including their visual numbers.
- Persistence omits default visual IDs (`VisualDisplay::non_default` returns
  `None` when `num == pos`), so v1 fixtures stay deterministic and re-save
  byte-for-byte; `claim_visual` rebuilds the per-type high-water counter and
  rejects duplicate numbers. Decode respects the store caps.
- UI surfacing reuses the existing row/detail/link structs rather than adding
  a parallel data path; the palette's `PaletteEntry` gained an explicit `name`
  so element matching can be by visual-ID prefix and name separately.
- No panics or `unsafe`; no new dependencies. `cargo clippy --all-targets
  -- -D warnings` and the full workspace tests pass.
