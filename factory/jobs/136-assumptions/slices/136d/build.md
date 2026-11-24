Commit: 519b632 Add AssumptionWithoutClaim lint, client relations, edits, inspector support, and i18n (136d)
Files:
- crates/aegis-model/src/lint.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/tests/assumptions.rs
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/configuration.rs
- crates/aegis-client/src/grid/edit.rs
- crates/aegis-client/src/inspector.rs
- crates/aegis-client/src/pages.rs
- crates/aegis-client/src/picker.rs
- crates/aegis-client/src/relations.rs
- crates/aegis-client/src/systems.rs
- crates/aegis-client/src/table.rs
- crates/aegis-client/src/tree_diff.rs
- crates/aegis-client/src/undo_label.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/tests/configuration.rs
- crates/aegis-client/tests/inspector_view.rs
- crates/aegis-client/tests/relations.rs
- crates/aegis-client/tests/table_conventions.rs
- crates/aegis-client/tests/table_features.rs
- crates/aegis-client/tests/undo_every_op.rs
- crates/aegis-client/tests/undo_labels.rs
- crates/aegis-workbench/src/app/workbench.rs
Done when:
1. `cargo test --locked -p aegis-model --test assumptions` -> exit 0 (18 passed; 0 failed, including `lint_assumption_without_claim_and_suppresses_unlinked_claim`).
2. `cargo test --locked -p aegis-client --test relations` -> exit 0 (7 passed; 0 failed, including `every_model_link_has_a_registry_row_and_vice_versa`, `many_matches_the_fixture`, `keys_and_labels_are_valid`, `new_keys_resolve_in_every_language`, and `assumption_excludes_candidates_omit_gates`).
3. `cargo test --locked -p aegis-client --test inspector_view` -> exit 0 (4 passed; 0 failed, including `assumption_inspector_relations_and_excluded_tree_step_causes`).
4. `cargo test --locked -p aegis-client --test table_conventions && cargo test --locked -p aegis-client --test undo_every_op` -> exit 0 (`table_conventions`: 9 passed; `undo_every_op`: 5 passed, covering `RemoveAssumption`, `RemoveClaim`, `RemoveTreeNode`, `RemoveTree`, `ReplaceTree` step move, and `BranchStepToGate`).
5. `cargo fmt --all && scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench` -> exit 0 (`fmt`, `clippy`, `test` passed for all 3 crates).
Notes:
- Non-test diff size: +563 / -29 lines (including +24 lines across the three `.ftl` locale files, +539 Rust source lines, below `max_lines: 580`).
- `crates/aegis-client/src/relations.rs` previously had no `ElementKind::ALL` constant; added `ElementKind::ALL` alongside `ElementKind::Assumption` and filtered `elements_of_kind(ElementKind::Step)` to exclude gates.
- `crates/aegis-client/tests/table_features.rs` was also updated at line 469 to include `Selection::Assumption(_)` in its exhaustive `Selection` match.
- `pages::page_of` and `pages::select` temporarily map `Selection::Assumption(_)` to `Page::Configuration` and no-op until `136e` adds `Page::Assumptions`.
NEXT: reviewer

## Rework 1
Commit: 42f2b35 Shrink assumption client diff below line cap (136d)
Files:
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/pages.rs
- crates/aegis-client/src/tree_diff.rs
- crates/aegis-client/src/undo_label.rs
Fix:
- Reduced non-test diff from 592 (`+563 / -29`) to 567 (`+545 / -22`) counted lines (cap 580) without adding `#[rustfmt::skip]` by extracting shared `assumption_excludes`/`set_excludes_op`/`set_member` helpers in `commands.rs`, simplifying `sync_excludes` in `tree_diff.rs`, tightening `Edit`/`ProblemRow`/`undo_label` arms, and reverting unrelated churn (`HashSet` import, `Edit::created` signature, and an existing comment in `pages.rs`).
Checks:
- `cargo fmt --all -- --check && scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench` -> exit 0 (all steps passed in 3 crates).
- `cargo test --locked -p aegis-model --test assumptions` -> exit 0 (18 passed; 0 failed).
- `cargo test --locked -p aegis-client --test relations` -> exit 0 (7 passed; 0 failed).
- `cargo test --locked -p aegis-client --test inspector_view` -> exit 0 (4 passed; 0 failed).
- `cargo test --locked -p aegis-client --test table_conventions` -> exit 0 (9 passed; 0 failed).
- `cargo test --locked -p aegis-client --test undo_every_op` -> exit 0 (5 passed; 0 failed).
NEXT: reviewer
