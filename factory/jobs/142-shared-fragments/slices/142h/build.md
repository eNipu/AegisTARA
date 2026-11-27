Commit: 33e99b6c9f5a028445439e526de7610469a32ed7 Add reversible bare-node kind changes retaining root and tree identities (142h)
Files: crates/aegis-model/src/tree.rs, crates/aegis-model/src/project.rs, crates/aegis-model/src/op.rs, crates/aegis-model/src/error.rs, crates/aegis-model/src/compat.rs, crates/aegis-model/src/store/op_log.rs, crates/aegis-model/src/store/wire.rs, crates/aegis-model/tests/common/mod.rs, crates/aegis-model/tests/ops_properties.rs, crates/aegis-model/tests/shared_fragments.rs, crates/aegis-client/src/undo_label.rs, crates/aegis-client/tests/engine_compatibility.rs, crates/aegis-client/tests/undo_every_op.rs
Done when:
1. All specified regression/property/workflow cases pass — `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace`: PASS (12 ops_properties, 35 shared_fragments, 26 store + 1 ignored, 24 workspace).
2. All specified regression/property/workflow cases pass — `cargo test --locked -p aegis-client --test undo_every_op --test engine_compatibility`: PASS (5 engine_compatibility, 6 undo_every_op).
3. Full workspace CI passes — `scripts/ci.sh`: PASS (all steps passed in 9 crate(s): fmt, clippy, test).
4. Both compatibility suites preserve historical success and current/future rejection after protocol3 — `cargo test --locked -p aegis-model --lib compat::tests` (5 passed) and `cargo test --locked -p aegis-client --test engine_compatibility` (5 passed): PASS.
Notes:
- Added `Op::SetTreeNodeKind { tree, id, gate, provenance }`, `AttackTree::set_node_kind(idx, gate)`, and `ModelError::BareTreeNodeRequired { tree, node }`.
- Shared `Project::has_node_metadata` between `check_remove_tree_node` and `check_set_tree_node_kind`; bare-node check rejects outgoing children, own feasibility/factors/via/attack_type, shared/fragment slots, mitigations, step condition links, review marks, and assumption `excludes_steps` references (so `assumption_invariants` and snapshot loading stay sound), while permitting root and multiply-parented nodes.
- Added strict `set_tree_node_kind` wire/view/read/write codec requiring `gate` (`null` or gate key) via `Option::deserialize`, updated `may_change_applicability`, `structure_tree`, `ProjectEvalCache::update`, `undo_label.rs`, and property generator (`ops_properties` kinds 82/79).
- Bumped `PROTOCOL_VERSION` and `MIN_SUPPORTED_PROTOCOL_VERSION` from 2 to 3 (`ENGINE_SEMANTICS_VERSION` remains 2, `FORMAT_VERSION` remains 1), retaining historical v1/protocol2/future-semantics fixtures and testing protocol 2 and protocol 4 headered rejections against current protocol 3.
- Production source lines changed (excluding `*/tests/*`): +268 / -20 (288 total) vs 400 cap.
NEXT: reviewer
