Commit: ca621b595c90b12f9ad08f19c55f6311e986cc25 Target shared steps in rating operations (141b)

Files:
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/inspector.rs
- crates/aegis-client/src/library.rs
- crates/aegis-client/src/sample.rs
- crates/aegis-client/src/suggest.rs
- crates/aegis-client/src/systems.rs
- crates/aegis-client/src/tree_diff.rs
- crates/aegis-client/src/undo_label.rs
- crates/aegis-client/src/view.rs
- crates/aegis-client/tests/attacker_profile.rs
- crates/aegis-client/tests/excluded_reasons.rs
- crates/aegis-client/tests/inspector_view.rs
- crates/aegis-client/tests/library_insert.rs
- crates/aegis-client/tests/relations.rs
- crates/aegis-client/tests/remove_loaded.rs
- crates/aegis-client/tests/tree_edit.rs
- crates/aegis-client/tests/undo_every_op.rs
- crates/aegis-model/benches/applicability.rs
- crates/aegis-model/benches/tree_nodes.rs
- crates/aegis-model/src/control.rs
- crates/aegis-model/src/lib.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/src/store/snapshot.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/suggest.rs
- crates/aegis-model/tests/applicability.rs
- crates/aegis-model/tests/applicability_cache.rs
- crates/aegis-model/tests/applicability_eval.rs
- crates/aegis-model/tests/assumptions.rs
- crates/aegis-model/tests/attacker_profile.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/control_settings.rs
- crates/aegis-model/tests/control_status.rs
- crates/aegis-model/tests/project.rs
- crates/aegis-model/tests/reduced_by.rs
- crates/aegis-model/tests/review_marks.rs
- crates/aegis-model/tests/second_methodology.rs
- crates/aegis-model/tests/shared_steps.rs
- crates/aegis-model/tests/step_via.rs
- crates/aegis-model/tests/store.rs
- crates/aegis-model/tests/store_limits.rs
- crates/aegis-model/tests/tree_nodes.rs
- crates/aegis-workbench/tests/inspector_view.rs

Done when:
- `cargo fmt --all` — pass.
- `cargo test --locked -p aegis-model --test shared_steps --test store_limits --test ops_properties` — pass (45 tests).
- `cargo test --locked -p aegis-client --test undo_every_op` — pass (6 tests).
- `cargo test --locked -p aegis-model --doc` — pass (772 tests).
- `cargo clippy --locked -p aegis-model -p aegis-client -p aegis-workbench --all-targets -- -D warnings` — pass.
- `scripts/ci.sh crates/aegis-model crates/aegis-client crates/aegis-workbench` — model fmt/clippy pass; client and workbench fmt/clippy/test pass; model test fails only in the four persistence-dependent `store_properties` cases listed below, pending 141d.

Notes:
- Counted source lines against base `62036ff59e787a2535ea1125fcd759e628e634e8`: 698 (461 additions + 237 deletions), within the 700-line cap. Tests, benches, docs, and lock files are excluded from this count.
- Known 141d sequencing limitation: shared entities are not yet persisted in snapshots or workspace files. The generators were not skipped or weakened.
- `cargo test --locked -p aegis-model --test store_properties` fails exactly: `snapshot_round_trip_first_methodology`, `snapshot_round_trip_second_methodology`, `replay_first_methodology`, and `replay_second_methodology`. Each reload loses the generated `SharedStep` arena entry.
- `cargo test --locked -p aegis-model --test workspace_properties` fails exactly: `workspace_round_trip_first_methodology` and `workspace_round_trip_second_methodology`. Each round-trip loses the generated `SharedStep` arena entry.
- Shared-target op-log encoding/decoding, all six malformed target combinations, exact undo/redo, factor compaction, blank-actor and unknown-shared-step guards, and shared-via removal guards pass.
- No push or merge performed.

NEXT: reviewer
