Commit: abbb968 Link attack steps to shared steps and evaluate through them (141c)
Files:
- crates/aegis-model/src/error.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/system.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/ops_properties.rs
- crates/aegis-model/tests/shared_steps.rs
Done when:
- cargo fmt --all: PASS
- cargo test --locked -p aegis-model --test shared_steps --test ops_properties: PASS (26 shared-step tests, 11 property tests)
- cargo clippy --locked -p aegis-model --all-targets -- -D warnings: PASS
- scripts/ci.sh crates/aegis-model: FAIL only at persistence properties; fmt and clippy PASS, 203 unit tests PASS, integration suites through shared_steps PASS, then store_properties reports 4 failures
- cargo test --locked -p aegis-model --test workspace_properties: FAIL (2 persistence round-trip tests), as expected before 141d
Notes:
- Counted source lines from git diff --numstat ca621b595c90b12f9ad08f19c55f6311e986cc25 over crates/aegis-model/src: 717 (additions + deletions), under the 720-line cap. Tests, docs, and lock files are excluded.
- Exact remaining failures are store_properties::{replay_first_methodology, replay_second_methodology, snapshot_round_trip_first_methodology, snapshot_round_trip_second_methodology} and workspace_properties::{workspace_round_trip_first_methodology, workspace_round_trip_second_methodology}. Shared steps and shared links are not persisted until slice 141d; no persistence test was skipped or weakened.
- ProjectEvalCache handles both forward link ops and the inverse ops passed by apply_cached by reconciling reverse indexes from current project state.
- No push or merge performed.
NEXT: reviewer
