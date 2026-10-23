# 0019. Lazy top-k attack paths

Date: 2026-10-23
Status: Accepted

## Context

`Evaluation::why_path` (ADR 0006) follows a single deciding spine from the root of an `AttackTree` down to one bottleneck leaf step. While that explains the single dominating step of the root value, analysts and automated treatment planners also need:

1. **Complete attack paths (minimal cut sets):** At an `AND` or `SAND` gate, an attacker must execute *every* child branch, not just the bottleneck branch. A complete attack path is a minimal set of leaf steps (`Node::Step`) whose joint execution satisfies the root goal.
2. **Ranked alternative paths (`top-k`):** After mitigating the #1 easiest attack path with a `SecurityControl`, the analyst needs to see the #2, #3, ..., #k easiest paths to verify whether nearby `OR` branches remain high-feasibility.
3. **Sub-exponential laziness (rule A4):** An `AND`-of-`OR` tree of depth $d$ can have $2^{\Omega(d)}$ distinct attack paths (for example, 20 binary `OR` stages under an `AND` gate produce $2^{20} = 1\,048\,576$ paths). Materializing all paths before sorting is infeasible.

## Decision

1. **`AttackPath<V>` (`paths.rs`).**
   - Represents one complete, minimal leaf-step realization of the root goal:
     - `steps: Box<[NodeIdx]>` — distinct leaf step indices in left-to-right pre-order traversal (always non-empty).
     - `bottleneck: NodeIdx` — the first step in `steps` that attains `feasibility`.
     - `feasibility: V` — the path feasibility ($\min_{s \in \text{steps}} v(s)$).
   - Holds no labels, names, or strings (rule R6).

2. **Best-first lazy iterator `AttackPaths<'a, V>` (`paths.rs`, `eval.rs`).**
   - `Evaluation::attack_paths(&self, tree: &AttackTree) -> Option<AttackPaths<'_, V>>` validates that `tree` structurally matches `self` in $O(V)$ time and initializes a max-heap (`BinaryHeap<SearchState<V>>`) with the normalized root state.
   - `SearchState<V>` expands deterministic nodes (`Node::Step`, `GateKind::And`, `GateKind::SeqAnd`) eagerly in pre-order until reaching the first unexpanded `GateKind::Or` gate (`next_or`), recording each visited node in `expanded: Vec<bool>` so shared DAG nodes are expanded at most once per state.
   - Each state's upper bound `bound: V` is the minimum of `eval.get(u)` across already-collected `steps`, `next_or`, and unexpanded nodes in `worklist`. Under `MaxMin<V>`, `eval.get(u)` equals the exact feasibility of the easiest completion of `u`, so `bound` is exact for the rank-`0` completion and monotonically non-increasing across expansions ($S'.\text{bound} \le S.\text{bound}$).
   - Heap ordering compares `(bound, Reverse(choices))` where `choices: Vec<u32>` records the rank (`0` = highest-valued child, earliest in stored order on ties) chosen at each expanded `Or` gate. Rank-`0` choices are therefore expanded depth-first before rank-`1` alternatives of the same bound, finding the `k = 1` path in $O(V(V + E))$ time without materializing sibling subtrees.
   - On DAGs with shared nodes (`has_shared`), complete states are checked for cut-set minimality (`is_minimal_cut_set`: removing any single step from `steps` leaves `tree.root()` unsatisfied) in $O(|S| \cdot (V + E))$ time and deduplicated by canonical sorted step set (`BTreeSet<Box<[NodeIdx]>>`).

3. **Convenience helpers on `Evaluation` and `Project` (`eval.rs`, `project.rs`).**
   - `Evaluation::top_k_paths(&self, tree: &AttackTree, k: usize) -> Option<Vec<AttackPath<V>>>`
   - `Project::top_k_tree_paths(&self, id: TreeId, k: usize)`
   - `Project::top_k_residual_tree_paths(&self, id: TreeId, k: usize)`
   - `Project::top_k_threat_paths(&self, id: ThreatId, k: usize)`
   - `Project::top_k_residual_threat_paths(&self, id: ThreatId, k: usize)`

## Consequences

- `top_k_paths` enumerates the $k$ easiest minimal attack paths lazily without materializing all $2^{\Omega(d)}$ paths (rule A4), verified on a $2^{20}$-path `AND`-of-`OR` tree in `< 1 ms`.
- Property tests (`random_dag_attack_paths_match_minimal_cut_set_oracle` in `tests/eval_properties.rs`) prove that on arbitrary DAGs with shared nodes, `attack_paths` yields the exact set of minimal cut sets in non-increasing order of feasibility, matching an independent power-set oracle.
