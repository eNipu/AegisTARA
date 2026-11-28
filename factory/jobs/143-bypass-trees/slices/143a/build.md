Commit: 26b37d2eeb05da8d07e552c2927cc5c9736c5e53 Add SecurityControl bypass trees with graph guards and persistence (143a)
Files:
- crates/aegis-model/src/control.rs
- crates/aegis-model/src/error.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/src/store/snapshot.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/workspace.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/ops_properties.rs
- crates/aegis-model/tests/store.rs
- crates/aegis-model/tests/workspace.rs
Done when:
1. `SecurityControl::bypass` and `Op::SetControlBypass` with full guard/cycle/depth checks and exact undo/redo — `cargo test --locked -p aegis-model --test ops_properties`: 12 passed, 0 failed
2. Snapshot, op-log, and workspace persistence round-trip `bypass`, omit when `None`, and blame `"shared/controls.json"` on invalid bypasses — `cargo test --locked -p aegis-model --test store --test store_limits --test workspace`: store 28 passed (1 ignored), store_limits 25 passed, workspace 25 passed, 0 failed
3. Model crate formats, passes clippy `-D warnings`, and passes all unit/integration tests — `scripts/ci.sh crates/aegis-model` (`ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test`) and `cargo test --locked -p aegis-model --test ops_properties --test store --test store_limits --test workspace --test control_status` (control_status 4 passed): all passed
Notes:
- Non-test diff outside `crates/aegis-model/tests/` is +282/-21 lines (within the 380-line cap).
- In `crates/aegis-model/tests/ops_properties.rs`, `coverage()` runs 4096 sequences (up from 2048) because bumping `seed()` from `0u8..92` to `0u8..93` shifted the deterministic ChaCha sequence so rare `SetDamageLinkRationale` hits needed 4096 runs to appear alongside all 83/80 op kinds.
- In `crates/aegis-model/tests/common/mod.rs` (`second_every_op`), `bypass_tree` is created and exercised after `RemoveAttackTree { id: tree }` so at most one attack tree exists at any op, preserving `workspace::layout_covers_every_snapshot_key`'s single-holder-per-key invariant.
NEXT: reviewer
