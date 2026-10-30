# Build — 118-visual-ids

## What was built

### Model (`aegis-model`)
- `id.rs`: a `VisualPrefix` enum for the 11 project element types (`A`, `D`,
  `T`, `AT`, `SC`, `G`, `CL`, `C`, `IF`, `DF`, `TB`), with `as_str()`,
  `format(n)` (`format!("{prefix}{n:02}")`, growing to 3+ digits at `100`),
  and a strict `parse_num` (prefix + 2..10 ASCII digits, no leading zeros on
  3+ digits), plus `format_visual_id`.
- `project.rs`: each `Arena` now keeps `visuals: Vec<u32>`, a per-type
  `next_visual` high-water counter, and a `removed_visuals: BTreeMap<Id, u32>`.
  `push` assigns monotonically and never reuses a deleted number; `remove`
  remembers the number so the `inverse` of `Op::Remove*` restores it; `Project::eq`
  compares live elements including their visual numbers. `Project` exposes
  `*_visual_num`/`*_visual_id` for every type and a `visual_id(ElementRef)`
  lookup.
- `store/snapshot.rs` + `store/wire.rs`: each element serializes a
  `visual_id` only when it is non-default (`VisualDisplay::non_default`, i.e.
  `num != pos`), so v1 golden fixtures load deterministically and re-save
  byte-for-byte. Loading reconstructs the per-type high-water counter with
  `claim_visual` (rejecting duplicate or reused numbers) and honours the
  store's decode caps and `only_keys_on_disk` rule.

### Client (`aegis-client`)
- `pages.rs`: every row/detail/link/choice struct carries a `visual_id`, and
  `palette_elements`/`PaletteElement` index every open element by visual ID,
  name and kind.
- `view.rs`: `ThreatRow`, `MitigationView`, `TreeView` and `Inspector` carry
  visual IDs, and inspector/title render paths include them.
- `state.rs`/`session.rs`: the open project view caches `palette_elements`.
- `report/{csv,doc,xlsx}.rs`: risk register, HTML/doc tables and XLSX sheets
  all surface the visual ID (a leading `ID` column or a `"A00 · name"` cell).

### Shell (`aegis-shell`)
- `controller.rs`: every element table gets a leading visual-ID column,
  detail/inspector titles and link lists include visual IDs, and the command
  palette indexes project elements.
- `shortcuts.rs`/`controller.rs`: the palette filters element entries by
  visual-ID **prefix** and by name (not substring), so `T00` finds the threat
  `T00` and not the attack tree `AT00`.
- `ui/app.slint`: the `ID` columns and inspector fields; translations updated.

## Fixes made while running checks
- Imported the `pages::*_rows`/`*_detail` helpers in
  `crates/aegis-client/tests/pages.rs` (the integration test referenced them
  unqualified).
- Palette visual-ID matching changed from substring to prefix so a visual-ID
  query is unambiguous (`T00` ≠ `AT00`).

## Verified
- `cargo fmt --all -- --check`.
- `scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-shell`
  (fmt, `clippy --locked --all-targets -- -D warnings`, `cargo test --locked`).
- `cargo test --workspace --locked`.
- `scripts/deny.sh` (advisories, bans, licenses, sources).

## Not built (remaining)
- None. The job covers all four acceptance criteria: the prefix scheme,
  monotonic/non-reuse/undo semantics, persistence with v1 fixture
  compatibility, and the UI/report/palette surfacing.
