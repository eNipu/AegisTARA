Commit: b03172b Add, rename and remove generic elements with op log (205b)
Files: crates/aegis-model/src/{element.rs,op.rs,project.rs,error.rs,lib.rs}, crates/aegis-model/src/store/{op_log.rs,wire.rs,mod.rs}, crates/aegis-model/tests/{elements.rs,common/mod.rs}, crates/aegis-client/tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test elements` -> 17 passed.
2. `cargo test --locked -p aegis-client --test undo_every_op` -> 2 passed.
3. `scripts/ci.sh crates/aegis-model crates/aegis-client` -> fmt, clippy, test all ok for both crates.
4. NOT MET: `git diff --numstat HEAD~1 -- crates/aegis-model/src` shows 463 added and 12 deleted lines (cap 400). About 110 lines of element.rs are doc comments and four doc tests the prompt asked for, plus the store itself. I did not cut them because the prompt requires them.
Notes:
- `ElementStore.kind_of` has `#[allow(clippy::zero_sized_map_values)]` because `ElementKind` has one variant today (clippy -D warnings failed otherwise).
- `commit_add_element` returns `Op` (not `Result`) for clippy `unnecessary_wraps`; project.rs wraps it in `Ok`.
- `validate` arms for the three ops `return` (skip the trailing provenance check), as the checks do their own provenance check; `RenameElement` has no provenance check, as the prompt's order lists none.
- `get_mut` still carries a leftover `#[allow(dead_code)]` although rename uses it; harmless, remove in 205c.
- Added one extra test `rename_refuses_a_bad_name` and `add_reads_back_the_element`.
- Client walk added as `walk_elements`; element ops are not in `second_every_op` or `record_every_op`.
NEXT: reviewer

## Rework 1

The gate failed only on size: 475 counted lines outside tests, cap 400. No code or test failed.

I tried to get under the cap without dropping anything the prompt requires: separate `use` lines instead of re-wrapped import groups, shorter doc tests, one merged check arm. That brought the count to about 436. It cannot reach 400 without removing required parts (the four doc-tested reads, the three inverse ops, or the op log wiring). The changes are discarded; HEAD is unchanged (`b03172b`).

Where the 475 lines are, outside tests: `element.rs` 293 (element and store about 125, the four public reads with doc tests about 70, checks and commits about 90), `op_log.rs` 64, `op.rs` 39, `wire.rs` 35, `project.rs` 26, `error.rs` 13, `store/mod.rs` 3, `lib.rs` 2.

Suggested split for the planner: 205b1 = the store, `Element`, the four reads, the three ops, checks and commits, and `Project` wiring (about 340 lines, with the client walk and `tests/elements.rs` minus the log tests). 205b2 = op log support (`VocabKind::ElementKind`, `ElementKindKey`, `OpWire`/`OpView`/`op_view`/`op_from_wire`/`within_caps`, about 100 lines) with `ops_round_trip_through_the_op_log` and `op_log_refuses_an_unknown_kind`. Note that `undo_every_op` may need the log arms to compile, since `op_view` and `op_from_wire` match every `Op` variant exhaustively; if so, put the minimal arms in 205b1 and the tests plus `ElementKindKey` in 205b2. The planner should check this.

NEXT: planner

## Rework 2

Fixed the review finding. In `validate` the element arm no longer `return`s; it propagates with `?`, so the trailing provenance check runs. `RenameElement` with a blank actor now gives `EmptyActor`. Added test `rename_refuses_a_blank_actor` in `tests/elements.rs` (checks `validate` matches `apply`). Source change: 1 line in `project.rs` (size unchanged). `scripts/ci.sh crates/aegis-model crates/aegis-client` passed (fmt, clippy, test).

NEXT: reviewer
