VERDICT: PASS
## For the owner
What changed: Added `crates/aegis-client/src/fragments.rs` implementing bounded `CopyMode::{PreserveReferences, Materialize}`, `CopyPlan` (`new_tree_ops`, `insert_ops`, `root_kind`), and `copy_subtree` to copy or materialize attack-tree DAG subtrees without flattening derived ratings.
Added `EditError::FragmentCopyLimit { resource, max }` in `crates/aegis-client/src/commands.rs`, exposed `#[doc(hidden)] pub mod fragments` in `crates/aegis-client/src/lib.rs` so external integration tests can exercise it before 142j/142k wiring, and added 10 integration tests in `crates/aegis-client/tests/shared_fragments.rs`.
Rework 1 deferred extra DAG link insertions in `build_plan_direct` until after owned node insertions so Pass 1 tracks only owned children, matching `CopyPlan::insert_ops`, and added regression tests for cross-gate linked siblings ahead of their extra parent and out-of-order owned siblings.
Look at:
- `crates/aegis-client/src/fragments.rs:335-409` — `build_plan_direct` Pass 1 owned `add_child`/`insert_child` placement followed by Pass 2 `link_at` wiring, preserving `AttackTree` child and parent orderings and parallel `meta` alignment.
- `crates/aegis-client/src/fragments.rs:273-333` — iterative memoized `preflight_materialize` enforcing `MAX_ATTACK_TREE_NODES`, `MAX_DAG_EDGES`, and `MAX_FRAGMENT_DEPTH` before allocation.
- `crates/aegis-client/src/fragments.rs:63-161` — `CopyPlan::new_tree_ops` and `CopyPlan::insert_ops` emitting skeleton creation, `AddTreeNode`/`LinkTreeNode` order, and leaf metadata ops without querying uncreated destination nodes.
Risk: `fragments` is `#[doc(hidden)] pub` rather than `pub(crate)` so `crates/aegis-client/tests/shared_fragments.rs` can test it before 142j/142k wire `Edit`; all bounds, iterative expansion, DAG edge/parent order, independent folding, and atomic rollback/undo-redo are verified by the 10 tests in `shared_fragments.rs`.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff 33e99b6c9f5a028445439e526de7610469a32ed7..HEAD
## Findings
none
## Lesson
[builder] Defer extra DAG link insertions until after owned-node insertions when building an AttackTree arena in topological order; otherwise a linked child placed ahead of its extra parent breaks sibling-based `insert_child` placement.
NEXT: human
