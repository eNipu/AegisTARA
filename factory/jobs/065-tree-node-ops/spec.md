# Spec — 065-tree-node-ops

## Title
Node-level attack tree operations, 4,096-node tree cap, and interactive node expansion (`FND-02`, `FND-03`)

## Design Reference
- Reference: `docs/adr/0014-store-trees-and-provenance.md`, `docs/security-and-architecture-review.md` (`FND-02`, `FND-03`)
- Affected crates/paths: `crates/aegis-model/src/{tree,op,project,error}.rs`, `crates/aegis-model/src/store/{op_log,snapshot,wire}.rs`, `crates/aegis-client/src/{tree_diff,commands,outline,view}.rs`, `crates/aegis-shell/ui/app.slint`, `crates/aegis-shell/src/controller.rs`

## Problem & Goal
Replace whole-tree replacement on node edits with six granular node-level operations (`AddTreeNode`, `RemoveTreeNode`, `LinkTreeNode`, `UnlinkTreeNode`, `RenameTreeNode`, `SetTreeNodeGate`) that preserve unchanged nodes, ratings, factors, and mitigations (`FND-02`), raise `MAX_ATTACK_TREE_NODES` from `32` to `4,096` while keeping single-op `MAX_TREE_OP_NODES = 32` (`FND-03`), reconcile outline edits via a minimum-weight diff (`tree_diff.rs`), and provide interactive node expansion at both leaf ends and intermediate gates in `aegis-shell`.

## Acceptance Criteria
1. Six node-level ops in `aegis-model` with exact inverses (preserving arena slot, child position, and extra-parent link order), `MAX_ATTACK_TREE_NODES = 4_096`, `MAX_TREE_OP_NODES = 32`, and O(1) end-of-arena append fast path.
2. `tree_diff.rs` reconciles any old and new `AttackTree` into minimal node-level ops using a Fenwick-tree Maximum-Weight Increasing Subsequence (`mwis_mask`) so unchanged nodes keep their IDs, ratings, factors, and controls.
3. Interactive node editing and expansion in `aegis-client` and `aegis-shell` (`RenameTreeNode`, `SetTreeNodeGate`, `AddTreeChild`, `BranchStepToGate`, `InsertParentGate`, `AddSiblingNode`, `MoveTreeNode`, `RemoveTreeNode`) work in one atomic undo batch without touching the text outline.
4. Benchmark (`benches/tree_nodes.rs`) confirms node ops on a 4,096-node tree complete in `0.1–0.2 ms` (well within the `5 ms` budget `A1`).
5. All unit, property, and integration tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
