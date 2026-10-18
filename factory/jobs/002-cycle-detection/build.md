## Round 1
Commits: 83e8fea Add DAG links and Kahn cycle detection to attack tree
Files:
- crates/aegis-model/src/topo.rs (new, private: Kahn + cycle extraction)
- crates/aegis-model/src/tree.rs (`link`, `topological_order`, invariant docs, `parent` doc, `check_invariants` updated for links, `mod links` unit tests)
- crates/aegis-model/src/error.rs (`DuplicateEdge { parent, child }`, `Cycle(Vec<NodeIdx>)` + Display helper)
- crates/aegis-model/src/lib.rs (crate docs, `mod topo`)
- crates/aegis-model/tests/cycles.rs (new: proptest vs reference DFS, deep chains)
- docs/adr/0002-dag-links-and-cycle-detection.md (new)
- docs/adr/0001-arena-and-typed-ids.md (pointer to 0002)

Acceptance criteria (all commands from crates/aegis-model):
- [x] AC1 — `cargo test link_shares_child` → 1 passed
- [x] AC2 — `cargo test link_errors` → 1 passed (8 error cases incl. precedence: both unknown → parent; step parent + unknown child → child; linked edge also duplicate; len + children of every node compared before/after, check_invariants)
- [x] AC3 — `cargo test link_allows_cycle` → 1 passed (cycle-closing edge to root + self-link)
- [x] AC4 — `cargo test topo_order_acyclic` → 1 passed (shared child, later gate linking earlier node; exact expected order; root first)
- [x] AC5 — `cargo test cycle_self_link` → 1 passed (`Cycle(vec![a])`, also root self-link)
- [x] AC6 — `cargo test cycle_path_exact` → 1 passed (`Cycle(vec![g1, g2, g3])`, nodes below excluded); extra test `cycle_path_rotated_to_smallest`
- [x] AC7 — `cargo test topo_deterministic` → 1 passed (Ok and Err, twice and on clone)
- [x] AC8 — `cargo test --test cycles` → 3 passed; proptest `topo_matches_reference`, 64 cases, 0..500 nodes, 0..=500 links, reference = iterative 3-color DFS. Half of cases use forward-only links (guaranteed acyclic) so both Ok and Err are exercised.
- [x] AC9 — `cargo test deep_chain` → 2 passed (100,000-gate chain Ok in order; with `link(last, root)` → Cycle of length 100,000, equal to the chain)
- [x] AC10 — `cargo test cycle_display` → 1 passed (`cycle in attack tree: 3 -> 7 -> 4 -> 3`)
- [x] AC11 — `cargo test --doc` → 30 passed (new doctests on `link`, `topological_order`, `ModelError::Cycle`); clippy clean. Each doc states time/space.
- [x] AC12 — `grep -nE '\.unwrap\(|\.expect\(|panic!' src/*.rs` → hits only in tree.rs lines ≥ 597 and id.rs lines ≥ 182, all inside `#[cfg(test)]` modules; topo.rs/error.rs have none. No slice indexing in non-test code (`get`/`get_mut` only).
- [x] AC13 — `grep -rn "never has cycles" src/ ../../docs/adr/0001-arena-and-typed-ids.md` → no output; ADR 0002 has context, options, decision, complexity.
- [x] AC14 — `git diff --stat master -- spikes docs/decisions.md docs/engineering-standards.md` → empty.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (37 unit, 3 properties, 2+1 cycles... all ok; 30 doctests)

Notes:
- Open questions resolved as the spec defaults: permissive `link`; `Cycle` holds `NodeIdx`; `parent()` stays the owning parent (doc updated to say so).
- `ModelError::Cycle` is a tuple variant `Cycle(Vec<NodeIdx>)` to match AC5 wording.
- Visit marks use `Vec<Option<usize>>` (walk position) and `Vec<Option<NodeIdx>>` (predecessor) instead of `fixedbitset`: positions are needed, not bits; no new dependency. Rationale in ADR 0002.
- In-degree is `Vec<u32>` with `saturating_add`; overflow needs a 2^32-node arena where every node links one child — commented in topo.rs.
- Avoided let-chains because `rust-version = "1.85"`.
- `check_invariants` (test-only) no longer requires every child's owning parent to be the gate holding it; it now checks child indices are in range and unique per gate.
- ADR 0002 status is "proposed" pending the approver's answer to Open question 1.

## Round 2
Commits: 5e70556 Reject cycle-closing links, use NodeIds in errors, add owner and parents
Files:
- crates/aegis-model/src/tree.rs (`link` rejects cycles; `parent()` → `owner()`; new `parents()`; `linked_parents: Vec<Vec<NodeIdx>>`; invariants extended; `mod links` tests rewritten)
- crates/aegis-model/src/topo.rs (returns index paths; new `find_path` iterative DFS; arena-level tests incl. AC8 property test and 100k cyclic chain)
- crates/aegis-model/src/error.rs (`Cycle(Vec<NodeId>)`, `DuplicateEdge { parent: NodeId, child: NodeId }`, new `WouldCreateCycle { parent, child, path: Vec<NodeId> }`)
- crates/aegis-model/src/lib.rs (crate docs)
- crates/aegis-model/tests/cycles.rs (proptest of `link` vs reference BFS reachability; deep chains)
- crates/aegis-model/tests/properties.rs (`parent` → `owner` rename only)
- docs/adr/0002-dag-links-and-cycle-detection.md (status accepted; Q1–Q3 decisions with reasons; bulk-import upgrade path)
- docs/adr/0001-arena-and-typed-ids.md (pointer sentence updated)

Reviews in round-1/: all PASS, no CHANGES items. This round applies rework.md (Rework 1).

Rework items:
- [x] Q1 `link` rejects cycle-closing edges — `topo::find_path` iterative DFS from `child` (stack = current path, `Vec<bool>` visited, no recursion, no new dep). Checked after `DuplicateEdge`. Tests (all `cargo test --lib links` → 16 passed):
  - self-link rejected: `link_rejects_self_link` (gate and root)
  - node under own descendant (A→B→C, `link(C, A)` → path [A, B, C]): `link_rejects_descendant`
  - root under any gate rejected: `link_rejects_root`
  - cycle through an earlier link rejected: `link_rejects_cycle_through_link`
  - diamond / shared subtree allowed: `link_allows_diamond`
  - shortcut to existing descendant allowed: `link_allows_shortcut`
  - rejected link leaves tree unchanged: every rejection goes through `assert_cycle_rejected`, which compares len, children and parents of every node, and runs `check_invariants`
  - proptest `link_matches_reference` (`cargo test --test cycles`, 64 cases, ≤500 nodes, ≤500 links): `link` succeeds exactly when a reference BFS says child does not reach parent (and edge new, parent a gate); on rejection the tree is unchanged and the path is valid (starts at child, ends at parent, consecutive edges, no repeats); afterwards topo order is valid and parents/children agree.
  - `deep_chain_rejects_cycle`: 100,000-gate chain, `link(last, root)` → `WouldCreateCycle` with the 100,000-ID path, no stack overflow; `link(root, last)` allowed.
- [x] `topological_order()` behavior unchanged (Kahn, same order/cycle rules). Since cycles can no longer be built through the public API, cycle-path tests moved to arena level in `src/topo.rs` (`cycle_self_link`, `cycle_path_exact`, `cycle_path_rotated_to_smallest`, `topo_deterministic_on_cycle`, `deep_chain_cycle`, proptest `topo_matches_reference` vs. three-color DFS with a forward-only mode for acyclic cases). Tree-level `cycle_self_link` / `cycle_path_exact` / `topo_deterministic` in `mod links` use a test-only `force_edge` that bypasses `link`, to check the `NodeId` mapping.
- [x] Q2 errors carry `NodeId`: `Cycle(Vec<NodeId>)` (edge order, starting with the node of smallest internal index, no repeat), `WouldCreateCycle { parent, child, path }`, `DuplicateEdge { parent, child }` as `NodeId`. Display uses hyphenated UUIDs; `cycle_display` asserts exact text for all three and that labels never appear. `UnknownNode` and `NotAGate` unchanged.
- [x] Q3 `parent()` → `owner()`; `parents(idx)` iterates owner first then linked parents in link order, O(number of parents), no allocation; stored per node in `Vec<NodeIdx>`, O(1) amortized push in `link`. Test `owner_and_parents` (link order ≠ index order, root empty, foreign index empty). Invariant check now verifies p ∈ parents(c) exactly once per edge, the reverse, and acyclicity; `invariants_catch_missing_parent_entry` and `cycle_self_link` show it detects violations.
- [x] ADR 0002 updated: status accepted, Q1–Q3 decisions with the owner's reasons, costs, bulk-import-with-Kahn upgrade path.

Acceptance criteria re-run (spec ACs, adapted where rework changed semantics):
- AC1 `cargo test link_shares_child` → pass (also checks `parents`)
- AC2 `cargo test link_errors` → pass (DuplicateEdge now with NodeIds)
- AC3 superseded by rework Q1: `link_allows_cycle` replaced by the `link_rejects_*` tests above → pass
- AC4 `cargo test topo_order_acyclic` → pass
- AC5/AC6/AC7 `cargo test cycle_self_link`, `cargo test cycle_path_exact`, `cargo test topo_deterministic` → pass (tree level with NodeIds, and arena level in topo.rs)
- AC8 `cargo test topo_matches_reference` (arena, src/topo.rs) and `cargo test --test cycles` → pass
- AC9 `cargo test deep_chain` → 4 passed (topo Ok on chain; arena cyclic chain → 100k-node Cycle; link rejection with 100k path; `find_path_deep_chain`)
- AC10 `cargo test cycle_display` → pass
- AC11 `cargo test --doc` → 32 passed (new doctests: `parents`, `WouldCreateCycle`, `Cycle`, updated `link`); clippy clean
- AC12 grep → hits only after `#[cfg(test)]` (topo.rs ≥190 vs mod at 181; tree.rs ≥703 vs 698; id.rs ≥182 vs 167)
- AC13 `grep -rn "never has cycles" src/ ../../docs/adr/0001-...` → nothing; ADR 0002 present
- AC14 `git diff --stat master -- spikes docs/decisions.md docs/engineering-standards.md` → empty

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (51 unit, 3 cycles, 2 properties, 32+1 doctests)

Notes:
- `link` error order: UnknownNode(parent), UnknownNode(child), NotAGate(parent), DuplicateEdge, WouldCreateCycle. Duplicate before cycle is cheap-first; in a DAG an existing edge can never also close a cycle, so the order is not observable otherwise.
- `link` cost doc: O(k + d + e) plus one zeroed O(V) allocation for visited flags (`vec![false; V]`, calloc); at most O(V + E). Stated honestly in the doc and ADR rather than claiming pure O(descendants).
- `Cycle` path order is "smallest internal index first" (deterministic per tree); IDs are mapped after rotation.
- The proptest for `link` draws random (parent, child) pairs; cycle rejections occur in most cases (any pick of an ancestor or self) but are not forced every case — the dedicated unit tests cover each edge case deterministically.
- Breaking API change: `AttackTree::parent` removed (renamed `owner`). Only in-crate callers and tests/properties.rs used it.
