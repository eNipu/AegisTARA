Commit: 4ea7108ab44f301e9fd96aef40bf1d3974501e83 Persist fragment references with exact inverses (142c)

Files:
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/compat.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/snapshot.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-model/tests/common/shared_fragment_guards.rs
- crates/aegis-model/tests/ops_properties.rs
- crates/aegis-model/tests/shared_fragments.rs
- crates/aegis-client/src/undo_label.rs
- crates/aegis-client/tests/engine_compatibility.rs
- crates/aegis-client/tests/undo_every_op.rs
- crates/aegis-server/tests/common/mod.rs
- crates/aegis-server/tests/events.rs
- crates/aegis-server/tests/login.rs
- crates/aegis-server/tests/login_switch.rs

Done when:
1. `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties --test store --test workspace` — PASS, 85 passed; one existing golden-fixture regeneration test ignored.
2. `cargo test --locked -p aegis-client --test undo_every_op --test engine_compatibility` — PASS, 11 passed.
3. `scripts/ci.sh` — PASS, workspace fmt, clippy and tests; all steps passed in 9 crates.
4. `cargo test --locked -p aegis-model --lib compat::tests` — PASS, 5 passed.

Notes:
- `cargo fmt --all` passed before final validation. Reviewed staged diff/status and committed only scoped code and tests using the configured identity and required co-author trailer.
- Completed the existing uncommitted slice implementation without discarding it. Preserved factory/board.json and all untracked 142b artifacts; this build artifact is not in the code commit. No planning or gate machinery changed.
- Production changed lines against 40fe73d97a91132f07a951cd2f6753b74cf40d8a: 301 additions plus deletions, including compat's inline unit tests; below the 560-line cap.
- Use/Unuse preserve source provenance and return inverses with supplied provenance. Snapshot/workspace reload and op-log replay preserve full projects and bytes; 256 parallel references round-trip in canonical NodeId order.
- Loader tests verify duplicate, malformed UUID, unknown field/target/node, wrong role, rated source, cycle, depth and bounded-vector cap refusals. Forward targets, multiple users, invalid actors, unlink/remove/undo, empty-field omission and existing byte-stable v1 fixtures pass.
- Fragment dependencies deliberately remain unrated/error results until 142d, even when their targets are rated. Use/Unuse rebuild the evaluation cache; unrelated ordinary trees still evaluate normally.
- Protocol/current minimum are 2; semantics and file format stay 1. Both compatibility suites separately cover historical v1 success, current headerless rejection, isolated historical protocol-2 rejection and dynamic future-semantics rejection.
- Initial workspace CI found two authenticated server test clients still omitting compatibility headers. Updated only test harnesses for current-contract headers (development, signed OIDC and bearer clients); focused server login/events tests and final full CI pass.
- Bounded fragment lists reuse Limit::AttackTreeNodes with maximum 256 rather than introducing an out-of-scope error variant.
- No unmet criteria.

NEXT: reviewer
