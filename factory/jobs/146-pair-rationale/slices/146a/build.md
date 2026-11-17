Commit: 1dca31061de46b563d3cc57f15ea1de5813b7314 Add damage-link rationale op and public-ID overlay (146a)
Files: crates/aegis-model/src/threat.rs, crates/aegis-model/src/op.rs, crates/aegis-model/src/project.rs, crates/aegis-model/src/store/op_log.rs, crates/aegis-model/tests/pair_rationale.rs (new)
Done when:
- `cargo test --locked -p aegis-model --test pair_rationale` -> exit 0; 6 tests pass, including the shuffled link+rationale-op test that asserts `assert_eq!(p1, p2)` and `assert_eq!(save(&p1), save(&p2))`.
- `cargo test --locked -p aegis-model --lib` -> exit 0; 197 tests pass, including `project_invariants_first_methodology` / `project_invariants_second_methodology` (the golden v1 byte tests live in the `store` target and are covered by the CI run below).
Before you commit (all run from the worktree root with CARGO_TARGET_DIR shared with the main checkout):
- `cargo fmt --all` -> exit 0.
- `scripts/ci.sh crates/aegis-model` -> `ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test`, "all steps passed in 1 crate(s)" (this includes `cargo test --locked` over every test target, so the golden v1 snapshot test passes).
- `cargo test --locked -p aegis-model --lib` -> exit 0 (197 passed).
- `cargo test --locked -p aegis-model --test pair_rationale` -> exit 0 (6 passed).
Gate size: `git diff --numstat <base>` over the four source files counts 157 changed lines (155 added, 2 deleted), well under `max_lines` 360.
Notes:
- BLOCKER, step 6 (op-log decode arm) and step 7 (random-op generator extension) cannot be done as written without touching `crates/aegis-model/src/store/wire.rs`. `OpWire` is defined at `wire.rs:2061` (its `SetImpactRationale` variant at `wire.rs:2458`); `op_log.rs` only imports it. Step 6 says "In `store/op_log.rs`, mirror ... `OpWire` and its decode arm", but the decode arm `OpWire::SetDamageLinkRationale => Op::SetDamageLinkRationale` needs a matching `OpWire` enum variant, which must be declared in `wire.rs`. 146a's allow list and its "Do not" list both forbid `store/wire.rs` (wire.rs belongs to slice 146b's allow list). I did NOT widen scope. Implemented in `op_log.rs`: the `OpView::SetDamageLinkRationale` variant, its `op_view` arm, and the `text` byte-cap arm. Not implemented: the `OpWire` variant + `op_from_wire` decode arm, and extending `tests/common/mod.rs` (its generator feeds `store_properties::op_round_trip`, which would fail to decode a written `SetDamageLinkRationale` record). Request: either add `crates/aegis-model/src/store/wire.rs` to 146a's allow list, or have planner move the op-log wire variant/decode arm into 146b (which already owns wire.rs) and drop steps 6's decode arm / 7's generator extension from 146a.
- The prompt's "add the new op to the metadata op list at line 12408 and to the arm at line 12501" is contradictory: both line numbers are arms of the same `match op` in `ProjectEvalCache::update` (`project.rs`), so one op cannot be in both without an unreachable pattern. I added it only to the metadata/no-op list (now `project.rs:12410`), which matches spec criterion 2 ("included in ... metadata-op list") and is correct: a rationale text change does not alter the eval cache's damage-threat reverse map.
- `pub struct DamageLink` is defined in the private `threat` module. Re-exporting it at the crate root would require `src/lib.rs`, which is outside 146a's allow list. It is still usable via `ThreatScenario::damage_links()` (the test reads `link.rationale` through inference without naming the type). If the API must be nameable as `aegis_model::DamageLink`, add `lib.rs` to the allow list.
- The over-cap text check returns `ModelError::DescriptionTooLong { max: MAX_DESCRIPTION_CHARS }`, not `InvalidText`; the nul-text case returns `InvalidText`. `check_text` (`description.rs:136`) distinguishes them, so the test asserts the exact error each guard returns (lesson L09).
- Overlay lookups use a linear scan because the overlay is sorted by public damage ID while `damage_rationale` receives a raw `DamageIdx`; a binary search on the raw index would be wrong when slot order differs from public-ID order.
NEXT: planner

## Rework 2

Commit: bba5cd9ab86fdb70896b47fecb5d46efe0e48b77 Persist damage-link rationale through the op log (146a)
Files: crates/aegis-model/src/store/wire.rs, crates/aegis-model/src/store/op_log.rs, crates/aegis-model/tests/common/mod.rs, crates/aegis-model/tests/pair_rationale.rs
Done when:
- `cargo test --locked -p aegis-model --test pair_rationale` -> exit 0; 7 tests pass (the new `set_damage_link_rationale_survives_op_log_round_trip` writes an op with non-empty text and a non-default `Provenance`, reads it back with `OpLogReader`, and asserts all four fields).
- `cargo test --locked -p aegis-model --test store_properties` -> exit 0; 8 tests pass, including both `op_round_trip_*` tests (the generator emits `Op::SetDamageLinkRationale`, so the new `OpWire` variant and `op_from_wire` decode arm are exercised).
- `cargo test --locked -p aegis-model --lib` -> exit 0; 197 tests pass.
Before you commit (all run from the worktree root with CARGO_TARGET_DIR shared with the main checkout):
- `cargo fmt --all` -> exit 0.
- `scripts/ci.sh crates/aegis-model` -> `ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test`, "all steps passed in 1 crate(s)". This also runs `ops_properties::ops_generator_covers_every_op_and_most_errors` (exit 0).
- `cargo test --locked -p aegis-model --lib` -> exit 0 (197 passed).
- `cargo test --locked -p aegis-model --test pair_rationale` -> exit 0 (7 passed).
- `cargo test --locked -p aegis-model --test store_properties` -> exit 0 (8 passed).
Gate size: cumulative `git diff --numstat b56ea68` over the five source files counts 175 changed lines (173 added, 2 deleted), well under `max_lines` 360.
Notes:
- Step 1 done: added `OpWire::SetDamageLinkRationale` after `SetImpactRationale` in `wire.rs` with the exact field order and serde attributes of the mirrored variant. No snapshot types were touched.
- Step 2 done: added the `op_from_wire` decode arm in `op_log.rs`. Verified the `OpView::SetDamageLinkRationale` variant, its `op_view` arm, and the `text` byte-cap arm from 1dca310 still exist and left them unchanged.
- Step 3 deviation (documented, not a blocker): the prompt asked for `self.rationale(s.c)` as `text`, but the shared `op()` generator feeds `store_properties::snapshot_round_trip` and `replay`, which compare `Project` after `save`/`load`. The snapshot wire format does not persist `damage_rationales` until slice 146b, so emitting a non-empty rationale from the shared generator makes those tests fail (`Project ==` differs on the loaded rationale). The arm therefore uses empty text (a clear, no-op write) so it still exercises `OpWire` encode/decode in `op_round_trip` without storing state the snapshot cannot yet persist. The dedicated `pair_rationale.rs` round-trip test uses non-empty text and a non-default `Provenance`, so all four fields — including provenance — are still proven to survive.
- Step 3 selector: used 65, the only value in `0u8..74` not claimed by an explicit arm (it was the sole input to the `_ => SetComponentSystem` catch-all). To keep `ops_properties::ops_generator_covers_every_op_and_most_errors` passing (it hardcodes the successful-variant count at 64), the arm falls back to that same `SetComponentSystem` op when the project has no linked threat/damage pair; only when a genuinely linked pair exists does it emit `SetDamageLinkRationale`. This preserves the prior coverage count.
- Step 4 done: new round-trip test in `pair_rationale.rs`.
- Step 5 verified, not redone: `Op::SetDamageLinkRationale { .. }` is present only in the metadata/no-op arm of `ProjectEvalCache::update` (`project.rs:12489`), beside `SetImpactRationale`/`SetFeasibilityRationale`; the `LinkDamage`/`UnlinkDamage` arm (`project.rs:12581`) is unchanged, as the rewritten prompt requires.
NEXT: reviewer
