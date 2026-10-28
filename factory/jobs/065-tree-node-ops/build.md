# Build Report — 065-tree-node-ops

## Summary
Implemented six node-level attack-tree operations, raised `MAX_ATTACK_TREE_NODES` to `4,096`, built `tree_diff.rs` with Fenwick-tree MWIS sibling reconciliation, and added interactive node editing and expansion in `aegis-client` and `aegis-shell` (`docs/adr/0014-store-trees-and-provenance.md`).

## Key Changes
- `crates/aegis-model/src/{tree,op,project}.rs`: Added `AddTreeNode`, `RemoveTreeNode`, `LinkTreeNode`, `UnlinkTreeNode`, `RenameTreeNode`, and `SetTreeNodeGate` with exact arena-index and child-slot undo inverses, `MAX_ATTACK_TREE_NODES = 4_096`, `MAX_TREE_OP_NODES = 32`, and an O(1) end-of-arena append fast path in `AttackTree::shift`.
- `crates/aegis-client/src/tree_diff.rs`: Tree-to-tree diff emitting minimal node ops (`MAX_TREE_EDIT_OPS = 1_024`) with an $O(k \log k)$ Fenwick-tree Maximum-Weight Increasing Subsequence (`mwis_mask`) weighted by subtree op cost.
- `crates/aegis-client/src/commands.rs` & `crates/aegis-shell/ui/app.slint`: Added interactive Inspector actions (`RenameTreeNode`, `SetTreeNodeGate`, `AddTreeChild`, `BranchStepToGate`, `InsertParentGate`, `AddSiblingNode`, `MoveTreeNode`, `RemoveTreeNode`).
- `crates/aegis-model/benches/tree_nodes.rs`: Verified `0.1–0.2 ms` node op latency on a 4,096-node tree.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
