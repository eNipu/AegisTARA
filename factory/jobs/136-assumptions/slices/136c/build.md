Commit: 9f93d93 Add assumption snapshot/workspace persistence and applicability integration (136c)
Files:
- crates/aegis-model/src/store/mod.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/snapshot.rs
- crates/aegis-model/src/store/workspace.rs
- crates/aegis-model/src/applicability.rs
- crates/aegis-model/tests/assumptions.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/workspace.rs
Done when:
1. `cargo test --locked -p aegis-model --test assumptions` -> exit 0 (17 passed, 0 failed), verifying exact hand-calculated feasibility/risk reductions and eliminations across `holds` toggles, own-condition suppression (`Reason::ConditionOff`), shuffled-order `ProjectEvalCache` soundness and order independence, byte-for-byte snapshot/workspace round-trips (including `#[serde(default)]` omitted `holds`), and L09 decode caps (`257` assumptions -> `Limit::Assumptions`, `65` claim assumptions -> `Limit::ClaimAssumptions`, `130 + 127 = 257` combined excludes -> `Limit::AssumptionExcludes`).
2. `cargo test --locked -p aegis-model --test workspace` -> exit 0 (19 passed, 0 failed), with `"assumptions"` covered by `layout_covers_every_snapshot_key`, `systems/main/assumptions.json` in `layout_paths_are_exactly_the_written_ones`, and assumption/claim-link ops exercised in `round_trip_every_kind`.
3. `cargo test --locked -p aegis-model --test ops_properties` -> exit 0 (11 passed, 0 failed).
Notes:
- `scripts/ci.sh crates/aegis-model crates/aegis-client` exited 0 (`fmt`, `clippy`, and `test` passed for both `aegis-model` and `aegis-client`, including `undo_every_op::a_whole_session_of_edits_undoes_back_to_empty`).
- Prompt mentions `StoreError::Limit(...)` in steps 2 and 5.4, which corresponds to the existing `StoreError::LimitExceeded(Limit::...)` variant used across `aegis-model` (`Limit::ASSUMPTION_EXCLUDES`, `Limit::Assumptions`, `Limit::ClaimAssumptions`).
- Prompt mentions `self.tree_id(t_idx)` in step 3.3, which corresponds to the existing `Project::attack_tree_id(t_idx)` method.
- `Project::may_change_applicability` and `ProjectEvalCache::apply_applicability_change` in `crates/aegis-model/src/project.rs` already handled `Op::SetAssumptionHolds`, `Op::SetAssumptionExcludes`, `touched_trees()`, and step-level exclusions, so no edits to `project.rs` were needed.
- Source-only diff against `a360ecb`: 235 added lines / 8 deleted lines across `crates/aegis-model/src/*` (under the 460-line cap).
NEXT: reviewer
