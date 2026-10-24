VERDICT: PASS

## Summary

Job `015-small-cleanups` resolves the five non-blocking follow-ups noted during jobs `002`, `008`, `010`, `011`, and `013`:
1. `AttackTree::from_wire_parts` is extracted into `crates/aegis-model/src/tree.rs:297-382` alongside `AttackTree::linked_parents_of` (`src/tree.rs:286-294`).
2. `find_path` in `crates/aegis-model/src/topo.rs:144-169` replaces its recursive helper with an explicit `Vec<(NodeIdx, usize)>` DFS frame stack.
3. `Op::SetThreatCategory` in `crates/aegis-model/src/project.rs:2656-2672` restores `self.refs.asset_properties` cleanly without `.expect()` if `bump_asset_properties` fails after the old category's properties were decremented.
4. `KeyOrder` in `crates/aegis-model/src/store/wire.rs:342-406` precomputes key-sorted rank tables once per `Project::save`, eliminating per-record `sort_by` allocations in `ImpactMap::save`, `PropertyKeySet::save`, and `CatalogRefWire::save_slice`.
5. `scripts/check.sh:21-22` verifies `cargo-llvm-cov --version` up front with an actionable `cargo install cargo-llvm-cov` hint.

## Acceptance Criteria Verification

- **AC1 (`find_path` iterative DFS, no recursion)**: Verified in `crates/aegis-model/src/topo.rs:144-169` and unit test `deep_chain_find_path` (`src/topo.rs:301-314`, 100,000-node chain). Time and space remain $O(V + E)$ with zero call-stack depth dependence.
- **AC2 (`Op::SetThreatCategory` panic-free rollback)**: Verified in `crates/aegis-model/src/project.rs:2656-2672` (`if let Err(err) = self.refs.bump_asset_properties(...) { let _ = self.refs.bump_asset_properties(..., -1); return Err(err); }`). No `.expect()` remains in `src/project.rs` outside `#[cfg(test)]`.
- **AC3 (`KeyOrder` precomputed rank tables on `Project::save`)**: Verified in `crates/aegis-model/src/store/wire.rs:342-406` (`KeyOrder::new(methodology)`), threaded through `ImpactMap::save`, `PropertyKeySet::save`, and `CatalogRefWire::save_slice` in `crates/aegis-model/src/store/snapshot.rs:135-182`. Canonical snapshot output is preserved byte-for-byte.
- **AC4 (`AttackTree::from_wire_parts` and `linked_parents_of`)**: Verified in `crates/aegis-model/src/tree.rs:286-382` with unit tests in `src/tree.rs:1661-1748` covering pure trees, shared DAG nodes, non-empty root parents, out-of-range indices, forward owners, and step children.
- **AC5 (`scripts/check.sh` upfront `cargo-llvm-cov` check)**: Verified in `scripts/check.sh:21-22`.

## Engineering Standards Audit

- **D1–D5 (Arenas & Typed IDs)**: `from_wire_parts` validates arena indices (`NodeIdx`) and `NodeId` uniqueness before constructing `AttackTree`.
- **D7 / P4 (Determinism)**: `KeyOrder` rank tables sort indices by key string once and break any tie by vocabulary index, producing identical canonical wire order.
- **A8 / A9 (Complexity & No $O(n^2)$)**: `KeyOrder::new` runs in $O(C \log C + P \log P + S \log S)$ once per `save`; `find_path` is $O(V + E)$ iterative.
- **Q3 (No `unwrap`/`expect`/`panic!` in library code)**: Verified across `src/topo.rs`, `src/tree.rs`, `src/project.rs`, and `src/store/wire.rs`.

## Findings

### Blocking
None.

### Non-Blocking
None.
