## Title

Threat × damage pair rationale (FND-09, job 146)

## Design Reference

- `docs/requirements.md` line 91 — FND-09.
- `docs/design/workbench-breakdown.md` line 193 — model and client parts are kept; the
  `.slint` screen is a separate, later decision.
- `docs/architecture.md` III.9.3 (lines 2269–2288), the element recipe. Parts 3 (wire),
  8 (page/inspector data), 10 (report/XLSX) and 11 (en/ja/pseudo) are in scope; parts 9 and
  12 (Slint, screenshots) are not.
- `docs/architecture.md` lines 2013–2014 (job 161 precedent: write the old single `goal`
  field when there is one so v1 files stay equal) and lines 2396–2402 (fields omitted when
  empty so v1 fixtures load unchanged).
- `factory/lessons.md` L06 (compare `save()` bytes, sort by public ID), L29 (resolve new
  reference fields by public ID in `Project::eq` **and** every `diff_arena` closure), L27
  (hyphenated Fluent ids), L24 (every construction site in the allow list).
- Job 147 pattern to reuse: `crates/aegis-model/tests/rating_rationale.rs`;
  `Op::SetImpactRationale` / `Op::SetFeasibilityRationale`; `MAX_DESCRIPTION_CHARS` /
  `DescriptionText` / `check_text`; `rationale` and `feasibility-rationale` Fluent keys.

## Problem & Goal

A threat-to-damage link says *that* a threat can cause a damage scenario but not *why* the
analyst made that link. A customer TARA work product, a reviewer and the report reader all
need the justification next to the link. Business value (rule P7): the rationale is a single
text attribute on an existing link, so the complexity is bounded — reuse the job 147 text
cap and naming, add one op, and keep the on-disk v1 shape whenever no rationale is set. No
new snapshot version, no migration, no user data is changed or deleted.

## Acceptance Criteria

1. `pub struct DamageLink { damage: DamageIdx, rationale: Arc<str> }` exists in
   `aegis-model`; `ThreatScenario::damage_links()` returns one per link, sorted by the
   target damage's public ID; `ThreatScenario::damage_rationale(DamageIdx)` returns `""`
   when unset. The existing `damages()` index accessor keeps working unchanged.
2. `Op::SetDamageLinkRationale { threat, damage, text, provenance }` validates with
   `check_text(text, MAX_DESCRIPTION_CHARS)`, rejects `ModelError::DamageNotLinked`,
   applies with an exact inverse (undo restores both the prior text and the prior
   provenance), and is included in the op-log size check and metadata-op list.
3. `Project::eq`/`same_threat` and the threat `diff_arena` closure compare link rationales
   resolved by **public ID** (L29). Setting or clearing a rationale marks only that threat
   modified in `Project::diff`; it changes no computed risk number.
4. Snapshot: with no rationale on any link, `damages` is serialized as the old array of ID
   strings. With any rationale set, it is serialized as an array of
   `{"damage": <id>, "rationale": <text>}` objects in the same public-ID order. Both forms
   decode; rationales are stored only for links that have one.
5. `golden_v1_snapshot_loads_and_resaves_byte_for_byte` still passes; re-saving the v1
   fixture writes no `rationale` key. A snapshot with rationales round-trips through
   `Project::save`/`load` with equal bytes and equal projects.
6. `SetDamageLinkRationale` round-trips through `OpCodec::encode`/`decode` and through
   `OpLogWriter`/`OpLogReader`, including the op-log byte cap.
7. Op-order test: applying `LinkDamage` and rationale ops in two different orders produces
   equal projects **and** equal `save()` bytes (L06).
8. Client data: `RiskRow.rationale` and `ThreatDetail.rationale` carry the deciding damage
   link's rationale (empty when there is no deciding damage). The risk table gets a
   `Rationale` column with key `rationale` and a cell that renders that text.
9. Report: `risk_rows_block` in the DOC report and the Threats sheet in XLSX each gain a
   Rationale column sourced from `RiskRow.rationale`. A client test asserts the header and
   the value appear.
10. Trilingual Fluent key `pair-rationale` in en, ja and pseudo; hyphenated id (L27).
11. No changes to `.slint` files, the shell controller, or headless screenshots.

## Out of scope

- Slint work, GPUI panes and screenshot tests (owner rule 2026-11-11); only client data and
  report/XLSX data ship here.
- A lint for a missing *pair* rationale. Job 147 already lints rated elements without a
  rationale; FND-09's lint requirement is met there.
- Changing `AddThreatScenario`, `LinkDamage` or `UnlinkDamage` op shapes, the snapshot
  magic/version, Excel import mapping, or AI suggestion text.

## Decisions

- New op `SetDamageLinkRationale`, not a new field on `LinkDamage`: every existing op line
  stays valid and the inverse is exact.
- The authoritative link set stays `ThreatScenario::damages` (a `Vec<DamageIdx>` sorted by
  public ID); rationales live in a sparse, parallel `Vec<(DamageIdx, Arc<str>)>` overlay and
  `DamageLink` is the public view type. Reason: zero churn for the 30+ index-only callers and
  the old v1 list form falls out for free when the overlay is empty.
- New snapshot object keys are `damage` (matching the op field) and `rationale`; `rationale`
  defaults to empty on decode but is always written in object form for deterministic bytes.
- Fluent key `pair-rationale` (not dotted): en `Pair rationale`, ja `組み合わせの根拠`,
  pseudo in the usual `⟦…␣⟧` wrapping.
- The column value is the rationale of the link to the *deciding* damage of that risk row
  (the damage already shown in `RiskRow.damage`); it is empty when there is no deciding
  damage. This keeps the risk table's existing one-row-per-threat shape.
- 146c's allow list adds `crates/aegis-client/tests/table_conventions.rs` and `crates/aegis-client/tests/undo_every_op.rs`: the new Rationale column changes the page's `ColumnSpec` (per-page spec test) and the new `SetDamageLinkRationale` op must appear in the undo-every-op test. L02/L24: a new op and a new column force their tests; add those files when the slice is cut, not after the gate fails.
- Round 2 keeps the op-log wire round-trip in 146a, not 146b: `store/wire.rs` is added to
  146a for the `OpWire::SetDamageLinkRationale` variant (its decode arm lives in the
  already-allowed `op_log.rs`), and the `tests/common/mod.rs` generator arm stays here, because
  an op that cannot persist is not a working op and the generator feeds `op_round_trip`;
  `lib.rs` is deliberately not added (criterion 1 needs `DamageLink` to exist in `aegis-model`
  and be returned by `damage_links()`, not to be nameable at the crate root), and `wire.rs`
  remains in 146b's allow list for the separate snapshot-form work.
