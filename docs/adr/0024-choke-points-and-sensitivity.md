# 0024. Choke Points, Hitting Set, and Tipping-Point Sensitivity

Date: 2026-04-19
Status: Accepted

## Context

In automotive TARA (ISO/SAE 21434 and UNECE R155), security analysts need two complementary analytical lenses beyond single-point risk evaluation and top-$k$ attack path enumeration (ADR 0019):

1. **Choke-point and hitting-set analysis** (`docs/brainstorm.md` §3.1): When an attack tree has multiple minimal cut sets (attack paths) realizing the root goal—especially in a DAG with shared sub-trees—analysts need to know which individual leaf steps appear across the most top-$k$ paths (`ChokePoint`), how often each step is the bottleneck of those paths, and what small set of leaf steps intersects every one of the top-$k$ easiest paths (`top_k_hitting_set`) so security controls can be placed for maximum coverage.
2. **Sensitivity and tipping-point analysis** (`docs/brainstorm.md` §3.4): Because feasibility and impact ratings during concept-phase and product-development TARA carry estimation uncertainty, analysts need to identify "fragile" ratings where a $\pm 1$ step change on a single attack step's feasibility, a direct threat feasibility, or a damage scenario's impact category flips the tree's root feasibility or the threat's worst-case `RiskLevel`.

All outputs must obey rule P4 (deterministic ordering across platforms and runs), rule R6 (never include user-supplied entity names or node labels in error or diagnostic structures), and rule A1 (use incremental DAG recomputation rather than cloning or re-evaluating the entire tree from scratch for each step perturbation).

## Decision

### 1. Choke Points (`ChokePoint<V>` and `Evaluation::choke_points`)
- Define `ChokePoint<V>` in `crates/aegis-model/src/paths.rs` (re-exported at the crate root) with accessors:
  - `step(&self) -> NodeIdx`: the leaf step's arena index.
  - `path_count(&self) -> usize`: number of paths among the top-$k$ minimal attack paths that contain `step`.
  - `bottleneck_count(&self) -> usize`: number of those paths where `step` is the bottleneck (`AttackPath::bottleneck`).
  - `easiest_path_feasibility(&self) -> &V`: feasibility of the easiest path containing `step`.
- `Evaluation::choke_points(&self, tree: &AttackTree, k: usize) -> Option<Vec<ChokePoint<V>>>` consumes at most $k$ paths from the lazy `attack_paths(tree)` iterator, accumulates per-step counts in a deterministic `BTreeMap<NodeIdx, (usize, usize, V)>`, and sorts the resulting vector by `(path_count desc, bottleneck_count desc, step asc)` (rule P4).
- Expose `Project` wrappers for both inherent and residual evaluations on trees and threats:
  - `Project::tree_choke_points`, `Project::residual_tree_choke_points`
  - `Project::threat_choke_points`, `Project::residual_threat_choke_points`

### 2. Greedy Top-$k$ Hitting Set (`Evaluation::top_k_hitting_set`)
- Finding a minimum-cardinality hitting set over $k$ minimal cut sets is equivalent to Set Cover (NP-hard in general), where the standard greedy algorithm (Chvátal 1979) achieves the optimal $H_k = \ln k + 1$ approximation ratio.
- `Evaluation::top_k_hitting_set(&self, tree: &AttackTree, k: usize) -> Option<Vec<NodeIdx>>` collects `top_k_paths(tree, k)` and iteratively selects the leaf step covering the maximum number of still-uncovered paths, breaking ties deterministically by smallest `NodeIdx` (rule P4), until all top-$k$ paths are hit.
- Expose `Project` wrappers for inherent and residual evaluations:
  - `Project::tree_hitting_set`, `Project::residual_tree_hitting_set`
  - `Project::threat_hitting_set`, `Project::residual_threat_hitting_set`

### 3. Single-Parameter $\pm 1$ Tipping-Point Sensitivity (`sensitivity.rs`)
- Create `crates/aegis-model/src/sensitivity.rs` (re-exported at the crate root) with four types:
  - `StepTippingPoint`: records a $\pm 1$ perturbation of a leaf step (`step: NodeIdx`, `node: NodeId`, `from_step: FeasibilityLevel`, `to_step: FeasibilityLevel`) that shifts an attack tree's root feasibility from `from_root` to `to_root`.
  - `FeasibilityTippingPoint`: records a $\pm 1$ perturbation of either an attack tree leaf step (`step: Some(NodeIdx)`, `node: Some(NodeId)`) or a threat's direct feasibility (`step: None`, `node: None`) that changes the threat's worst-case `max_risk` from `from_risk` to `to_risk`.
  - `ImpactTippingPoint`: records a $\pm 1$ perturbation of a linked damage scenario's impact rating (`damage: DamageId`, `category: ImpactCategory`, `from_impact: Level`, `to_impact: Level`) that changes the threat's worst-case `max_risk` from `from_risk` to `to_risk`.
  - `ThreatSensitivity`: bundles `feasibility`, `max_risk`, `feasibility_tipping_points`, `impact_tipping_points`, and `is_stable()`.
- **Incremental tree perturbation via `TreeEvalCache`**:
  - `Project::entry_tipping_points` builds one `TreeEvalCache` over the attack tree in $O(V + E)$ time, then tests each leaf step's $\pm 1$ adjacent `FeasibilityLevel`s using `TreeEvalCache::recompute_steps` ($O(\text{depth} \times \text{fan-out})$ per perturbation) and restores the step's baseline value immediately afterward.
- **$O(1)$-per-entry impact perturbation via top-2 risk tracking**:
  - When computing `ImpactTippingPoint`s across all rated `(damage, category)` entries of a threat, `Project::impact_tipping_points_for` precomputes the highest entry risk `max1`, its multiplicity `max1_count`, and the strictly second-highest risk `max2` in a single linear pass.
  - For each entry, the maximum risk across all *other* entries is `max2` when `entry_risk == max1 && max1_count == 1`, and `max1` otherwise. Each $\pm 1$ impact perturbation therefore computes the new worst-case `to_risk = other_max.max(new_entry_risk)` in $O(1)$ time, accurately handling tied deciding entries without false downward tipping points.

## Consequences

- **Analytical coverage**: Analysts and downstream CLI/TUI/GUI views can immediately surface shared bottleneck steps across top-$k$ paths, minimal control placement sets, and fragile $\pm 1$ feasibility or impact ratings for both inherent and residual risk.
- **Performance**: Tree tipping-point analysis reuses `TreeEvalCache::recompute_steps` rather than cloning or re-evaluating the full DAG per step, and impact tipping-point analysis evaluates each candidate perturbation in $O(1)$ time.
- **No name leakage (Rule R6)**: All choke-point and sensitivity structs identify entities exclusively by typed arena indices (`NodeIdx`) and UUIDv7s (`NodeId`, `DamageId`) plus methodology scale indices.
