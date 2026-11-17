# Build 134c

Commit: cb13168 Re-evaluate only the steps and threats a condition switch touches (134c)
Files:
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/applicability.rs
- crates/aegis-model/Cargo.toml (`[[bench]] applicability` only)
- crates/aegis-model/benches/applicability.rs (new)
- crates/aegis-model/tests/applicability_cache.rs (new)

Done when:
1. `cargo test --locked -p aegis-model --test applicability_cache`: 4 passed (locality, shuffled switches + undo vs full cache, order independence, applicability coverage over the general op generator).
2. `cargo test --locked -p aegis-model --test ops_properties`: 11 passed.
3. `cargo bench --locked -p aegis-model --bench applicability`: `set_condition_present` median 521.4 us (slowest 1.125 ms); `rating_edit_with_condition_off` median 624.7 ns. Both are well under 5 ms.
4. `scripts/ci.sh`: all steps passed (fmt, clippy, test), exit 0.

Notes:
- Counted source lines (verify.py does not count tests or benches): 219 (project.rs +204/-5, applicability.rs +16, Cargo.toml +4). The cap is 360.
- `may_change_applicability` is an exhaustive `const fn` match with no `_` arm. It is true for exactly the 27 ops listed in the prompt.
- `structure_tree(op)` returns the tree of a structure op (AddTreeNode, RemoveTreeNode, LinkTreeNode, UnlinkTreeNode, SetTreeNodeGate, AddAttackTree, RemoveAttackTree). `apply_applicability_change` skips that tree, because the op's own arm then rebuilds it under the new applicability, which is already swapped in. So no index-based work runs against a reshaped tree. Any other tree whose node count no longer matches its old or new `excluded` vector is rebuilt with `update_tree_structure`, which rebuilds the slot and resyncs its threats with `full_rebuild = true`.
- Step 3 deviation (necessary): the prompt says to visit "every tree in old or new `trees` map". A tree excluded only at tree level (`OwnerExcluded`, because all its threats are excluded) has no `trees` entry, so it would be missed. The new `Applicability::touched_trees()` therefore also yields `ElementRef::Tree` keys from `elements`. The other new helper is `node_flags(tree)`. Both are `pub(crate)`.
- Threats whose exclusion flipped are recomputed with `build_threat_eval` after the tree work, adding 2 to `recomputed_threats`.
- Mutation check: disabling the dirty-step update made the locality and shuffled tests fail, and disabling the threat recompute made the shuffled test fail. Both mutations were reverted.
- The bench fixture appends nodes with `AddTreeNode` at index = current length (no arena shift). It builds in a few seconds, outside the timed region.
- The fastest `set_condition_present` sample (about 53 us) is the switch back on, where the all-present shortcut in `Project::applicability` applies.
NEXT: reviewer
