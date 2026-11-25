Commit: 284d1d41c5ec7d0d94942c2fc485c20986045d39 Persist shared steps in snapshots and workspaces (141d)
Files:
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/store/mod.rs
- crates/aegis-model/src/store/snapshot.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/workspace.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/shared_steps.rs
- crates/aegis-model/tests/store.rs
- crates/aegis-model/tests/store_limits.rs
- crates/aegis-model/tests/workspace.rs
Done when:
- cargo fmt --all: pass
- cargo test --locked -p aegis-model --test store --test workspace --test store_limits --test shared_steps --test store_properties --test workspace_properties --test ops_properties: pass (126 passed, 1 pre-existing ignored fixture-regeneration test)
- scripts/ci.sh crates/aegis-model: pass (fmt, clippy -D warnings, full crate tests)
- Counted source lines: 282 (263 added, 19 removed), within the 460-line cap; tests and docs excluded.
Notes:
- Shared steps replay after items and before trees, descriptions, and translations; one unwrapped provenance clone is reused for AddSharedStep and all shared-target setters.
- Sparse visual_id overrides use VisualDisplay::non_default and claim_visual, preserving Lxx IDs across out-of-order IDs and deleted gaps.
- Linked-tree serializers read only own_step_* fields. An all-linked tree still emits the existing empty step_feasibility object; linked node IDs are absent from own-rating maps while step_shared is preserved.
- Workspace inner-wire decode errors remain blamed to aegis.json because files are merged before snapshot decoding; model-level shared-step errors route to shared/steps.json as requested.
NEXT: reviewer
