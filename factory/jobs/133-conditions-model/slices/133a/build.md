# Build 133a

Commit: 007c5649a39109ef04f24d15c75c6d816cf8bce8 Add typed Condition element with ids, arena, ops and codecs (133a)

Files:
- `crates/aegis-model/src/applicability.rs` (new): `MAX_CONDITIONS = 256`, typed `Condition { name, description, present, provenance }` with accessors and doc tests.
- `crates/aegis-model/src/id.rs`: `ConditionIdx`/`ConditionId` via `entity_ids!`, `VisualPrefix::Condition` with `"CF"`, doc assertions for `CF00`/`CF100`.
- `crates/aegis-model/src/lib.rs`: module, public exports of `Condition`, `MAX_CONDITIONS`, `ConditionId`, `ConditionIdx`.
- `crates/aegis-model/src/op.rs`: four ops plus inverse table, provenance and `with_provenance` matches, removal doc list.
- `crates/aegis-model/src/error.rs`: `DuplicateConditionId`, `UnknownCondition`, `ConditionNameEmpty`, `ConditionNameTooLong`, `TooManyConditions` (each with a doc test, IDs only in error text).
- `crates/aegis-model/src/project.rs`: arena field + init, accessors, validate/commit arms, condition-name/description/cap/duplicate/unknown checks, exact remove inverse, `diff_arena`, `Project::eq`, eval-cache no-op arm.
- `crates/aegis-model/src/diff.rs`: `ProjectDiff::conditions` field, accessor, `is_empty()` count.
- `crates/aegis-model/src/store/wire.rs`: four `OpWire` variants (`WireId<ConditionId>`, `Name`, `DescriptionText`).
- `crates/aegis-model/src/store/op_log.rs`: four `OpView` variants, `within_caps` handling, metadata name entry, `Op` → `OpView` and `OpWire` → `Op` arms.
- `crates/aegis-model/tests/conditions.rs` (new): ids/visual ids, exact inverses by `save()` bytes, all five new guards plus description/provenance guards, 256/257 cap, `ProjectDiff::conditions()`, 101-condition op-log round trip including `CF100`.
- `crates/aegis-model/tests/common/mod.rs`: the hand-written every-op walk applies and undoes the four condition ops without recording them (see Notes).
- `crates/aegis-client/tests/undo_every_op.rs`: `walk_conditions` covers the four new variants in the declared-variant check.

Done when:
1. `cargo test --locked -p aegis-model --lib id` — ok (34 passed; the `CF00`/`CF100` assertions are doc tests, also green in the full run).
2. `cargo test --locked -p aegis-model --test conditions` — ok (6 passed; inverse-by-`save()`-bytes, guards, cap, diff, 101-condition log round trip).
3. `cargo test --locked -p aegis-model --test store` — ok (20 passed, 1 ignored); no changes to store fixtures or byte layout.
4. `cargo test --locked -p aegis-client --test undo_every_op` — ok (2 passed; all four condition variants appear in `seen`).

Before you commit (all pass):
- `cargo fmt --all` — ok.
- `scripts/ci.sh crates/aegis-model crates/aegis-client` — ok (fmt, clippy -D warnings and all tests for both crates).
- The four "Done when" commands above — all ok.

Notes:
- `RemoveCondition` is intentionally unguarded in this slice; the `ConditionInUse` guard lands in 133b. Its inverse already restores name, description, present and provenance exactly.
- No snapshot fields, link ops, side table, step links or risk-number changes were added, per scope.
- The condition ops are exercised in the model's hand-written every-op walk (`second_every_op`) with an immediate undo and are deliberately **not** recorded in `record_every_op`. Recording them would break `tests/workspace.rs` round trips because conditions are not persisted until 133c; adding them to the random generator would change the hard-coded `ok_kinds` count in `tests/ops_properties.rs`, which is in 133c's allow list. Per the plan, 133c adds the generator arms and updates those counts. The four variants are covered now by `tests/conditions.rs` and the client `undo_every_op` walk.
- Op-log/wire round-trips for the four ops live in `tests/conditions.rs` (store.rs is not in this slice's allow list), so the store test command verifies nothing regressed.
- Changed source lines (excluding tests/docs): 580 insertions, 22 deletions, under the 600-line cap (L12); no `rustfmt::skip` (L28).

NEXT: reviewer
