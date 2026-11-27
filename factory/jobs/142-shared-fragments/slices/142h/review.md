VERDICT: PASS
## For the owner
What changed:
Added `Op::SetTreeNodeKind` and `AttackTree::set_node_kind` to switch a bare node (including root or multiply-parented nodes) between a step and an empty gate while keeping its ID, label, index, owner, incoming edges, and tree role.
Enforced a strict bare-node guard (`ModelError::BareTreeNodeRequired`), updated evaluation cache invalidation and undo labels, and added a strict `set_tree_node_kind` wire/op-log codec requiring an explicit `gate` (`null` or gate key).
Bumped `PROTOCOL_VERSION` and `MIN_SUPPORTED_PROTOCOL_VERSION` from 2 to 3 (semantics 2, format 1) and updated model, store, property, undo, and compatibility tests.
Look at:
- crates/aegis-model/src/project.rs:9415 — `has_node_metadata` and `check_set_tree_node_kind` verify every per-node attachment (including `excludes_steps`) and check provenance before mutation without rejecting root or multi-parent nodes.
- crates/aegis-model/src/tree.rs:724 — `AttackTree::set_node_kind` swaps only the node slot after verifying zero outgoing children, preserving ID, label, index, owner, and parent edge lists in O(1).
- crates/aegis-model/src/store/wire.rs:2494 — `OpWire::SetTreeNodeKind` uses `Option::deserialize` without `default` so missing `"gate"`, invalid gate keys, and unknown fields are rejected; `crates/aegis-model/src/compat.rs:30` bumps protocol/minimum to 3.
Risk:
A bare-node check missing a node-keyed attachment (such as assumption `excludes_steps`) could violate snapshot/invariant soundness when turning a step into a gate, and a missing cache or compatibility update could leave stale fragment evaluations or accept old protocol writes. Checked by inspecting all `TreeEntry` and `Project` node-keyed collections, verifying every attachment and blocker rejection in `shared_fragments.rs`, validating incremental vs full cache and independent oracle agreement on fragment trees, and checking historical/current/future compatibility tests across protocol 3.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff ef86fe981b6e18ffa8c4fc26e020a1471bae5fb7..HEAD
## Findings
none
## Lesson
none
NEXT: human
