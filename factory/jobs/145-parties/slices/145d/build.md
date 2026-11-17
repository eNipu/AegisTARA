Commit: 43caab9d6cec3ce91ca56e83fd00ee97ca87bc4b Expose party responsible links across client data and tables (145d)

Files:
- `crates/aegis-client/src/view.rs` — `Selection::Party(PartyId)` import and variant; `RESPONSIBLE_G`/`RESPONSIBLE_C` Links columns (key `responsible`, width 160) after the name on Goals/Claims.
- `crates/aegis-client/src/relations.rs` — `GOAL_RESPONSIBLE`/`CLAIM_RESPONSIBLE` keys, `ElementKind::Party`, `kind_of` arm, forward reads over `responsible()` resolving indices to `PartyId`, two `many = true` registry rows with typed link/unlink functions, `elements_of_kind` party scan (O(elements), documented), plus a `Party` variant in the kind-of test.
- `crates/aegis-client/src/commands.rs` — `Edit::LinkGoalParty`/`LinkClaimParty`, `to_ops` emitting one `Op::Link*Party`/`Unlink*Party`, and `created()` arms selecting the goal/claim.
- `crates/aegis-client/src/pages.rs` — `GoalRowOf.responsible` (`P00 — Bosch`, stored order) filled in `goal_rows`/`claim_rows`; `PartyLink` and `GoalDetail.responsible` for the inspector picker; `Party` arms in `selection_visual_id`, `named`, `page_of` (Goals) and `select`.
- `crates/aegis-client/src/table.rs` — `responsible` cell joins the row's strings with `", "` on Goals/Claims; all other Links columns untouched.
- `crates/aegis-client/src/undo_label.rs` — `Party` selection-existence arm, `target_of` arms for all four new ops, and readable `parts()` labels (link/unlink, never the `"change"` catch-all).
- `crates/aegis-client/src/systems.rs`, `crates/aegis-client/src/picker.rs` — `Party` arms (`None` for systems, `party_idx`/`party_visual_id`/`party.name()` option text).
- `crates/aegis-client/src/lib.rs` — lib-level test: `MultiLinkPicker` on `GOAL_RESPONSIBLE` yields `Edit::LinkGoalParty`; applying and undoing restores `save()` bytes.
- `crates/aegis-client/locales/{en,ja,pseudo}/main.ftl` — `responsible`, `relation-goal-responsible`, `relation-claim-responsible`, `party-{us,customer,supplier,other}` (hyphenated ids; ja `責任者`).
- `crates/aegis-client/tests/relations.rs` — fixture with two parties linked to a goal and a claim, `Claim` added to `sources_of`, both rows checked forward and in reverse, unique keys, locale keys for all three languages.
- `crates/aegis-client/tests/table_conventions.rs` — shown cells and editable keys updated for the new column; new test for its key/kind/label/width/visibility.
- `crates/aegis-client/tests/undo_labels.rs` — new test covering all four party-link ops, their labels and target selections.

Done when:
1. `cargo test --locked -p aegis-client --test relations` — ok (5 passed; both new rows reachable forward and in reverse).
2. `cargo test --locked -p aegis-client --test table_conventions` — ok (9 passed; Responsible column on both tables).
3. `cargo test --locked -p aegis-client --lib relations` — ok (6 passed; all locale keys resolve).
4. `cargo test --locked -p aegis-client --test undo_labels` — ok (5 passed; four new ops covered).
5. Before commit: `cargo fmt --all` ok; `scripts/ci.sh crates/aegis-client` ok (fmt, clippy `-D warnings`, full crate tests); all four checks above re-run green.
6. Extra: `cargo test --locked -p aegis-client --lib` — ok (74 passed, including the new picker/save-bytes test).

Notes:
- Slice 145c introduced `LintCode::ShareTreatmentWithoutOtherParty` without a client-side `pages.rs::problem` arm, so the branch did not compile at the start of this slice. I added a minimal `problem()` arm for that lint in `pages.rs` (an allowed file) so the crate builds; no model, snapshot or risk data changed.
- `PartyId` is a stable public id but the model keeps `PartyIdx -> PartyId` resolution private, so the client resolves responsible indices by scanning `project.parties()`; this is the same documented O(elements) cost as the other registry scans.
- `select(ui, Selection::Party(_))` is a no-op and `selection_systems` returns `None` for parties: there is no party UI field or system membership yet. `page_of` maps parties to `Page::Goals` as required.
- Changed lines: 532 insertions, 13 deletions (545 total), under `max_lines` 620. All edits are inside the slice's `allow` list; no dependency, model op, snapshot format, risk number, `.slint` or `aegis-shell` change.
NEXT: reviewer
