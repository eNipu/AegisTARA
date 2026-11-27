Commit: 9e9c0c0427bfe48193fc483a4b74379e94359662 Add bounded subtree copy and materialization helpers (142i)
Files:
- crates/aegis-client/src/fragments.rs
- crates/aegis-client/src/lib.rs
- crates/aegis-client/src/commands.rs
- crates/aegis-client/tests/shared_fragments.rs
Done when:
- All specified regression/property/workflow cases pass: `cargo test --locked -p aegis-client --test shared_fragments` -> 9 passed; 0 failed; 0 ignored.
- Scoped CI passes: `scripts/ci.sh crates/aegis-client` -> ok (aegis-client fmt, clippy --all-targets -- -D warnings, and test all passed; 94 unit tests + all integration tests passed; `cargo fmt --all` clean).
Notes:
- Visibility note for reviewer: `142i.md` Do item 1 mentions internal `CopyMode` and `pub(crate) copy_subtree` while Do item 6 and Done when require testing them via `crates/aegis-client/tests/shared_fragments.rs` (an external integration test crate) and leaving `MakeShared`/`Unshare` unwired until 142j/142k (`clippy --all-targets -- -D warnings` rejects unused `pub(crate)` items in lib builds). Following `crates/aegis-client/src/folder.rs` precedent, `lib.rs` exposes `#[doc(hidden)] pub mod fragments;` with `#[doc(hidden)] pub` on `CopyMode`, `CopyPlan`, and `copy_subtree`.
- Both `preflight_materialize` and `build_plan_direct` use bounded iterative walks (explicit work stacks, `MAX_ATTACK_TREE_NODES`, `MAX_DAG_EDGES`, `MAX_FRAGMENT_DEPTH`, `MAX_FRAGMENT_REFERENCES`, `MAX_CONDITIONS_PER_ELEMENT`, `MAX_STEP_MITIGATIONS`); `EditError::FragmentCopyLimit { resource, max }` reports exact limit resource and cap.
- Production diff total excluding `tests/`: 594 lines (`commands.rs` +8, `lib.rs` +2, `fragments.rs` +584) <= 600 cap, zero `#[rustfmt::skip]`, zero `unwrap`/`expect`/`unsafe` in library code.
NEXT: reviewer

## Rework 1
Commit: 532863a0f072511a608348d99f76ef91077bcf06 Defer extra DAG links after owned insertions in copy_subtree (142i)
Files:
- crates/aegis-client/src/fragments.rs
- crates/aegis-client/tests/shared_fragments.rs
Done when:
- All specified regression/property/workflow cases pass: `cargo test --locked -p aegis-client --test shared_fragments` -> 10 passed; 0 failed; 0 ignored.
- Scoped CI passes: `scripts/ci.sh crates/aegis-client` -> ok (`aegis-client fmt`, `aegis-client clippy`, `aegis-client test` all passed; `cargo fmt --all` clean).
Notes:
- Fixed `build_plan_direct` (`crates/aegis-client/src/fragments.rs`) by separating Pass 1 (owned `add_child`/`insert_child` tracking only owned children in `cur`) from Pass 2 (deferred `link_at` calls), matching `CopyPlan::insert_ops`. Because `NodeIdx::try_from_usize` is `pub(crate)` in `aegis-model`, `insert_child` requires an existing node's `NodeIdx`; keeping only owned children in `cur` during Pass 1 guarantees `cur[owner_pos][after + 1].1` is an owned child of `owner_idx` whose arena index strictly exceeds `owner_idx`.
- Added regression test `cross_gate_linked_sibling_ahead_of_owner_preserves_order_and_folds` in `crates/aegis-client/tests/shared_fragments.rs` covering the 7-node DAG (`root -> [Q, O, R]`, `Q -> [K, Y]`, `O -> [Z, K]`, `R -> [Z]`) in both `CopyMode::PreserveReferences` and `CopyMode::Materialize` for `new_tree_ops` and `insert_ops`, plus an out-of-order owned-sibling DAG (`O -> [C1, C2]`, `C2 -> X`, `X -> C1`).
- Production diff total across `33e99b6c9f5a028445439e526de7610469a32ed7..HEAD` excluding `tests/`: 599 lines (`commands.rs` +8, `fragments.rs` +589, `lib.rs` +2) <= 600 cap.
NEXT: reviewer
