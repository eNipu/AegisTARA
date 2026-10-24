# Job 025: Incremental Evaluation Cache (`025-incremental-evaluation-cache`)

## Goal
Implement a dependency-tracked incremental evaluation cache (`TreeEvalCache<V>` and `ProjectEvalCache`) in `crates/aegis-model` so that a single rating or mitigation edit recomputes only the affected attack tree nodes and dependent threat risks within the 5 ms performance budget on the `large` (`100,000`-node) reference project, while keeping derived scores separate from source data (rules A1, A2, D10, P1, P4, R1, R2).

---

## Scope & Architecture

### 1. Separation of Derived Data from Source Data (Rule D10)
- `AttackTree` and `Project` remain the sole sources of truth. Their equality, cloning, and serialized representations never store cached evaluation results.
- Derived evaluations live in two explicit cache types that can be dropped and rebuilt at any time:
  - [`TreeEvalCache<V>`] in `crates/aegis-model/src/eval.rs` (re-exported at the crate root).
  - [`ProjectEvalCache`] in `crates/aegis-model/src/project.rs` (re-exported at the crate root).

### 2. Incremental DAG Evaluation (`TreeEvalCache<V>` in `eval.rs`)
- **Fields**:
  - `eval: Evaluation<V>`: memoized bottom-up node values, derivations, and root value.
  - `rank: Box<[u32]>`: topological position `0..V` for each `NodeIdx` such that `rank[parent.index()] < rank[child.index()]` for every directed edge `parent -> child`.
  - `edge_count: usize`: total outgoing child edges across all nodes when the cache was built, used to detect structural edits.
- **Algorithm (`recompute_steps`)**:
  1. Validates and deduplicates the input `steps` in descending topological rank order, evaluating `leaf(step)` once per unique step before mutating `self.eval` (strong exception safety on `UnknownNode` or `MissingLeafValue`).
  2. For each step whose value changed (`new_val != old_val`), updates its value in `self.eval` and pushes `(rank[parent], parent)` for each `parent in tree.parents(step)` into a max-heap `BinaryHeap<(u32, NodeIdx)>`.
  3. Pops dirty ancestor gates in strictly descending `rank` order (deepest descendants first, skipping duplicate entries for the same gate). Because `rank[parent] < rank[child]` for every edge, every changed descendant of gate `u` is guaranteed to be updated before `u` is popped, so `u` is folded at most once (rule A2).
  4. At each popped gate `u`, folds `tree.children(u)` using the `AttributeDomain`, updates `self.eval.derivations[u]`, and compares `new_val` with `old_val`.
  5. **Value-change early cutoff**: because a parent gate's `fold` reads only its children's values (never their derivations), if `u`'s value did not change (`new_val == old_val`), none of `u`'s parents can change either their value or their derivation, so propagation stops at `u`. If `new_val != old_val`, `u`'s parents are pushed onto the max-heap.
  6. Returns `Ok(recomputed_nodes)` (the number of steps and gates re-evaluated).
- **Complexity**:
  - Single-step rating change: time $O((1 + A) \log(1 + A) + \sum_{u \in A} (\text{children}(u) + \text{parents}(u))) = O(\text{depth} \times \text{fan-out})$, where $A$ is the set of ancestor gates whose input values changed.
  - Produces an `Evaluation<V>` (including `values`, `derivations`, `root_value`, `why_path`, and `explain`) bit-for-bit identical to running `evaluate(tree, domain, leaf)` from scratch.

### 3. Cross-Entity Dependency Graph (`ProjectEvalCache` in `project.rs`)
- **Fields**:
  - `trees: Vec<Option<CachedTreeEval>>`: per `TreeIdx` slot, holds `inherent: Result<TreeEvalCache<FeasibilityLevel>, ModelError>` and `residual: Result<TreeEvalCache<FeasibilityLevel>, ModelError>`.
  - `threats: Vec<Option<CachedThreatEval>>`: per `ThreatIdx` slot, holds `inherent: Result<ThreatRisk, ModelError>` and `residual: Result<ThreatRisk, ModelError>`.
  - `tree_threats: Vec<Vec<ThreatIdx>>`: reverse dependency index mapping each `TreeIdx` to the `ThreatIdx`s currently linked to it.
  - `damage_threats: Vec<Vec<ThreatIdx>>`: reverse dependency index mapping each `DamageIdx` to the `ThreatIdx`s currently linking to it.
  - `recomputed_nodes: u64` and `recomputed_threats: u64`: observability counters tracking how many tree nodes and threat risk evaluations were recomputed by incremental updates.
- **Incremental `Op` Propagation (`ProjectEvalCache::update` & `Project::apply_cached`)**:
  - **Metadata / asset / control / goal / claim ops** (24 of 37 `Op` variants): 0 tree nodes and 0 threat risks recomputed ($O(1)$).
  - **`Op::SetStepFeasibility` / `Op::SetStepFactor`**: incrementally recomputes the changed step and affected ancestor gates in both the inherent and residual `TreeEvalCache` for `tree`. For each threat in `tree_threats[tree]`, syncs only the touched `NodeIdx`s into `ThreatRisk::tree_evaluation` via `Evaluation::sync_nodes` in $O(|\text{touched}|)$ time (zero heap allocations, no $O(V)$ clone!) and updates `ThreatRisk::entries` in place only if the root feasibility changed.
  - **`Op::SetStepMitigation`**: leaves inherent tree and threat evaluations untouched ($0$ work); incrementally updates only the residual `TreeEvalCache` for `(tree, node)` and syncs dependent threats' residual `ThreatRisk`s.
  - **`Op::SetImpact`**: leaves all `TreeEvalCache`s untouched ($0$ tree nodes recomputed); updates `entries` and `deciding` in place only for the threats in `damage_threats[damage]` ($O(|\text{linked\_threats}| \times d \times c)$).
  - **`Op::SetThreatFeasibility` / `Op::SetThreatAttackTree` / `Op::LinkDamage` / `Op::UnlinkDamage`**: leaves all `TreeEvalCache`s untouched; updates only the target threat's reverse links and its inherent/residual `ThreatRisk`.
  - **`Op::AddAttackTree` / `Op::RemoveAttackTree` / `Op::AddThreatScenario` / `Op::RemoveThreatScenario` / `Op::AddDamageScenario` / `Op::RemoveDamageScenario`**: updates only the added/removed entity's slot and reverse index links.
- **Query API**:
  - `evaluate_tree(&self, project: &Project, id: TreeId) -> Result<&Evaluation<FeasibilityLevel>, ModelError>`
  - `evaluate_residual_tree(&self, project: &Project, id: TreeId) -> Result<&Evaluation<FeasibilityLevel>, ModelError>`
  - `evaluate_threat(&self, project: &Project, id: ThreatId) -> Result<&Evaluation<FeasibilityLevel>, ModelError>`
  - `evaluate_residual_threat(&self, project: &Project, id: ThreatId) -> Result<&Evaluation<FeasibilityLevel>, ModelError>`
  - `evaluate_risk(&self, project: &Project, id: ThreatId) -> Result<&ThreatRisk, ModelError>`
  - `evaluate_residual_risk(&self, project: &Project, id: ThreatId) -> Result<&ThreatRisk, ModelError>`
  - All query methods return borrowed references in $O(\log n)$ time (UUIDv7 lookup + slot borrow) with zero allocations.

### 4. Verification & Benchmarks
- **Unit & Integration Tests**:
  - `TreeEvalCache`: single-step and multi-step updates, early cutoff when a sibling dominates an OR/AND gate, tie-breaking derivation changes when root value stays constant, error rollback on `MissingLeafValue` and `UnknownNode`, fallback on structural tree edits.
  - `ProjectEvalCache`: incremental updates across all `Op` categories (`SetStepFeasibility`, `SetStepFactor`, `SetStepMitigation`, `SetImpact`, `SetThreatFeasibility`, `SetThreatAttackTree`, `LinkDamage`, `UnlinkDamage`, `Add*`/`Remove*`, and metadata no-ops), verifying `recomputed_nodes()` and `recomputed_threats()` counts and exact equivalence with uncached `Project::evaluate_*`.
  - Large 100,000-node (`large`) performance test asserting that a single rating change and risk query complete well within the 5 ms budget.
- **Property Tests (`proptest`)**:
  - Random DAGs with multiple rounds of random step rating mutations: `TreeEvalCache::recompute_steps` produces an `Evaluation` identical to fresh `evaluate()`.
  - Random `Project` op sequences (plus inverse undo sequences): `ProjectEvalCache` maintained via `apply_cached` produces identical results for `evaluate_tree`, `evaluate_residual_tree`, `evaluate_threat`, `evaluate_residual_threat`, `evaluate_risk`, and `evaluate_residual_risk` across all trees and threats as fresh uncached calls.
- **Benchmarks (`benches/eval.rs`)**:
  - Add Divan benchmarks for single-step incremental recomputation at `100,000` (`large`) and `1,000,000` (`stress`) nodes.
