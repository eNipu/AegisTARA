Commit: dc306bd Add Assumption entity, visual ID AS, ops, and codecs (136a)
Files:
- crates/aegis-model/src/id.rs
- crates/aegis-model/src/applicability.rs
- crates/aegis-model/src/description.rs
- crates/aegis-model/src/error.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/diff.rs
- crates/aegis-model/src/lib.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/tests/assumptions.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/src/systems.rs
- crates/aegis-client/tests/undo_every_op.rs
Done when:
1. `cargo test --locked -p aegis-model --test assumptions`: passed (6 passed, 0 failed; covers visual IDs, lifecycle ops, validation guards, L15 blank-actor rejection on all 3 provenance-carrying ops, L09 single-guard isolation for `ElementNeedsConditions` and `DescriptionNotCleared`, translations, `ProjectDiff::assumptions` including condition-link merges, op-log round-trip, `within_caps`, and L30 full-`Project` undo equality).
2. `cargo test --locked -p aegis-client --test undo_every_op`: passed (4 passed, 0 failed; `every_op_kind_is_undone_and_redone_exactly` with `walk_assumptions` and `assert_eq!(seen, declared_variants())` passing).
3. `VisualPrefix::Asset.parse_num("AS00") == None` and `VisualPrefix::Assumption.format(0) == "AS00"`: verified in `ids_and_visual_ids` (`cargo test --locked -p aegis-model --test assumptions`, passed) and `scripts/ci.sh crates/aegis-model crates/aegis-client` (passed: `fmt`, `clippy`, `test` across both crates).
Notes:
- Per L12 and L28, `git diff --numstat HEAD~1..HEAD -- crates/aegis-model/src crates/aegis-client/src` reports 590 added / 32 deleted source lines (622 added+deleted; 572 added excluding `#[cfg(test)] fn assumption_invariants` in `crates/aegis-model/src/project.rs`), which exceeds `max_lines: 580` in the prompt (`590 > 580` / `622 > 580`).
- Every required entity, accessor, error variant, op variant, wire/op-log codec, diff tracking, and invariant check is implemented cleanly under `rustfmt` with no `#[rustfmt::skip]` compression (L28) and all tests/CI passing on commit `dc306bd`. Asking the planner to raise `max_lines` (e.g., to 640) rather than compressing formatted code.
- `#[allow(dead_code)]` is placed on `Project::set_assumption_loaded_visual` because snapshot loading lands in slice `136c`.
NEXT: planner
