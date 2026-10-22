# Build: `016-store-trees-and-provenance`

## Changes
- `crates/aegis-model/src/tree.rs`:
  - Added `pub const MAX_ATTACK_TREE_NODES: usize = 32;`.
  - Added `pub(crate) type WireNodeParts`, `AttackTree::linked_parents_of`, and `AttackTree::from_wire_parts` to reconstruct and validate `AttackTree` instances in O(V + E) time and space.
- `crates/aegis-model/src/error.rs` & `crates/aegis-model/src/project.rs`:
  - Added `ModelError::TooManyAttackTreeNodes { tree: TreeId, max: usize }` (`"attack tree {tree} would have more than {max} nodes"`) and enforced `tree.len() <= MAX_ATTACK_TREE_NODES` in `Project::check_add_tree`.
- `crates/aegis-model/src/store/mod.rs` & `crates/aegis-model/src/lib.rs`:
  - Added `MAX_ACTOR_BYTES: usize = 1024`, `Limit::ActorBytes { max }`, `Limit::AttackTreeNodes { max }`, `Limit::StepFeasibility { max }`, and `VocabKind::FeasibilityLevel`, re-exporting `MAX_ACTOR_BYTES` and `MAX_ATTACK_TREE_NODES` at the crate root.
- `crates/aegis-model/src/store/wire.rs`, `src/store/snapshot.rs`, `src/store/op_log.rs`:
  - Added wire encoding and decoding for attack trees (`nodes`, `step_feasibility`), threat `attack_tree` links, the 4 tree operations (`AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`, `RemoveAttackTree`), and non-default `Provenance` on all entities and creating/mutating operations while omitting optional/default fields to preserve v1 byte-for-byte compatibility.
- `crates/aegis-model/tests/*.rs`, `crates/aegis-model/benches/store_memory.rs`, `docs/adr/0014-store-trees-and-provenance.md`:
  - Extended unit, integration, cap, and property tests, added the `attack_trees` memory benchmark gate, and recorded ADR 0014.

## Verification
- `./scripts/ci.sh`
- `cargo test --manifest-path crates/aegis-model/Cargo.toml --benches`
- `./scripts/deny.sh`
