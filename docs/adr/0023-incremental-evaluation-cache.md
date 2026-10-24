# 0023. Incremental Evaluation Cache

Date: 2026-04-19
Status: Accepted

## Context

Engineering standards rule D10 requires that derived data (scores, traces, indexes) be kept separate from source data and marked as a cache that can be dropped and rebuilt. Rule A1 requires incremental recomputation so that a rating change re-evaluates only the changed nodes, their ancestors in the attack tree DAG, and their dependents across modules, with a complexity of $O(\text{depth} \times \text{fan-out of changed path})$ and a wall-clock budget of $\le 5\text{ ms}$ on a `large` (`100,000`-node) project.

Prior to this change, `evaluate` (`crates/aegis-model/src/eval.rs`) and `Project::evaluate_*` (`crates/aegis-model/src/project.rs`) always evaluated attack trees and threat risks from scratch in $O(V + E)$ time, allocating a fresh `Evaluation<FeasibilityLevel>` on every call. While a full 100,000-node evaluation takes ~6 ms (ADR 0005/0006), calling `evaluate_risk` plus `evaluate_residual_risk` after each interactive edit or across multiple threats linked to large trees re-runs the full $O(V + E)$ traversal and clones the 100,000-node `Evaluation` into each `ThreatRisk`.

## Decision

### 1. Explicit, Rebuildable Cache Types (`TreeEvalCache<V>` and `ProjectEvalCache`)
- Keep `AttackTree` and `Project` free of cached evaluation state so they remain the sole source of truth (rule D10) and do not pay evaluation or cloning overhead during oplog replay or snapshot loading.
- Introduce two cache structs:
  - [`TreeEvalCache<V>`] in `eval.rs`: caches `Evaluation<V>` alongside each node's topological rank `rank: Box<[u32]>` (`rank[parent] < rank[child]` for every edge `parent -> child`).
  - [`ProjectEvalCache`] in `project.rs`: caches inherent and residual `TreeEvalCache<FeasibilityLevel>` per `TreeIdx`, inherent and residual `ThreatRisk` per `ThreatIdx`, and reverse dependency lists `tree_threats: Vec<Vec<ThreatIdx>>` and `damage_threats: Vec<Vec<ThreatIdx>>`.

### 2. Rank-Ordered Bottom-Up Propagation with Value-Change Cutoff (`TreeEvalCache<V>`)
- When one or more step ratings change via `TreeEvalCache::recompute_steps`:
  1. All unique input steps are validated and their new leaf values are queried before mutating the cached `Evaluation<V>`, preserving strong exception safety if any step is unknown or unrated (`MissingLeafValue`).
  2. Each step whose value changed updates its slot in `Evaluation<V>` and pushes `(rank[parent], parent)` for each parent in `tree.parents(step)` into a max-heap `BinaryHeap<(u32, NodeIdx)>`.
  3. Dirty ancestor gates are popped in strictly descending `rank` order (deepest nodes first) with duplicate suppression. Because `rank[parent] < rank[child]` along every directed edge, all changed descendants of a gate `u` are guaranteed to be processed before `u` is popped, so `u` is folded at most once (rule A2).
  4. Because `fold` depends only on children's values (never on children's derivations), a gate `u` whose folded value equals its previous value (`new_val == old_val`) updates its own `Derivation` (in case an earlier/later child tie shifted) and stops propagating to `tree.parents(u)`.
- This guarantees that the incrementally updated `Evaluation<V>` (including `values`, `derivations`, `root_value`, `why_path`, and `explain`) is bit-for-bit identical to a full `evaluate()` from scratch.

### 3. In-Place Cross-Module Risk Synchronization (`ProjectEvalCache`)
- `ProjectEvalCache` tracks two reverse dependency relations:
  - `tree_threats[tree_idx]`: threats whose `attack_tree` is `Some(tree_idx)`.
  - `damage_threats[damage_idx]`: threats whose `damages` list contains `damage_idx`.
- When a step rating or mitigation changes on `tree_idx`:
  - `SetStepMitigation` updates only the residual `TreeEvalCache` and residual `ThreatRisk`s, leaving inherent evaluations untouched.
  - `SetStepFeasibility` and `SetStepFactor` update both inherent and residual caches.
  - `TreeEvalCache` records the exact list of touched `NodeIdx`s (`touched`). For each dependent threat in `tree_threats[tree_idx]` that already holds a cached `Ok(ThreatRisk)`, `Evaluation::sync_nodes` copies only those $|\text{touched}| = O(\text{depth})$ slots into `ThreatRisk::tree_evaluation` in place—avoiding an $O(V)$ clone of the 100,000-node `Evaluation`—and recomputes `ThreatRisk::entries` in place only if the root feasibility changed.
- When `Op::SetImpact` or `Op::LinkDamage` / `Op::UnlinkDamage` occurs:
  - No `TreeEvalCache` is touched ($0$ tree nodes recomputed).
  - Affected threats update `ThreatRisk::entries` and `deciding` in place while preserving their existing `ThreatRisk::tree_evaluation` allocation.
- All 24 metadata, asset, control, goal, and claim operations that do not affect feasibility or impact ratings are $O(1)$ no-ops on `ProjectEvalCache`.

## Consequences

- **Performance** (`cargo bench --bench eval` on the `MaxMin<u8>` reference DAGs):
  - `large` (`100,000` nodes, `101,904` edges): full `evaluate_max_min` median **2.118 ms** vs `incremental_single_step` median **122.7 ns** (~17,000x faster, and ~40,000x below the $\le 5\text{ ms}$ budget).
  - `stress` (`1,000,000` nodes, `1,018,611` edges): full `evaluate_max_min` median **22.05 ms** vs `incremental_single_step` median **124.7 ns** (~176,000x faster, $O(\text{depth})$ independent of total node count).
  - `AttackTree` stores `link_count: u32` in the 4-byte padding adjacent to `root: NodeIdx` (zero increase in `size_of::<AttackTree>()`), enabling $O(1)$ topology-change detection via `AttackTree::edge_count`.
- **Zero-allocation reads**: `ProjectEvalCache::evaluate_*` returns borrowed `&Evaluation<FeasibilityLevel>` and `&ThreatRisk` references in $O(\log n)$ time with zero heap allocations (rule R1).
- **Determinism**: Property tests verify that arbitrary sequences of `Op`s and inverse undo `Op`s applied through `Project::apply_cached` yield results identical to fresh `Project::evaluate_*` calls on every tree and threat (rules P1, P4).
