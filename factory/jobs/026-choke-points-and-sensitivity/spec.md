# Job 026: Choke Points, Hitting Set, and Tipping-Point Sensitivity (`026-choke-points-and-sensitivity`)

## Goal
Implement attack-path choke-point ranking (`ChokePoint<V>`), greedy minimum hitting-set computation (`top_k_hitting_set`), and $\pm 1$ single-parameter tipping-point sensitivity analysis (`StepTippingPoint`, `FeasibilityTippingPoint`, `ImpactTippingPoint`, `ThreatSensitivity`) across `AttackTree` evaluations and `Project` inherent/residual risk determinations in `crates/aegis-model` (rules A1, A4, A8, P4, Q5, R6).

---

## Scope & Architecture

### 1. Choke Points (`ChokePoint<V>` in `paths.rs` & `Evaluation::choke_points` in `eval.rs`)
- **`ChokePoint<V>`**:
  - `step(&self) -> NodeIdx`
  - `path_count(&self) -> usize`
  - `bottleneck_count(&self) -> usize`
  - `easiest_path_feasibility(&self) -> &V`
- **`Evaluation::choke_points(&self, tree: &AttackTree, k: usize) -> Option<Vec<ChokePoint<V>>>`**:
  - Inspects the top-$k$ minimal attack paths from `self.attack_paths(tree)`.
  - Counts how many top-$k$ paths contain each leaf step, how many have that step as their bottleneck, and records the feasibility of the first (easiest) path containing the step.
  - Sorts deterministically by `(path_count desc, bottleneck_count desc, step asc)` (rule P4).

### 2. Greedy Top-$k$ Hitting Set (`Evaluation::top_k_hitting_set` in `eval.rs`)
- **`Evaluation::top_k_hitting_set(&self, tree: &AttackTree, k: usize) -> Option<Vec<NodeIdx>>`**:
  - Collects `self.top_k_paths(tree, k)` and greedily selects the leaf step that intersects the largest number of still-uncovered paths, breaking ties by lowest `NodeIdx` (rule P4), until all top-$k$ paths are intersected.

### 3. Tipping-Point Sensitivity Analysis (`sensitivity.rs` & `project.rs`)
- **`StepTippingPoint`**:
  - `step() -> NodeIdx`, `node() -> NodeId`, `from_step() -> FeasibilityLevel`, `to_step() -> FeasibilityLevel`, `from_root() -> FeasibilityLevel`, `to_root() -> FeasibilityLevel`.
  - Computed via `TreeEvalCache::recompute_steps` in $O(\text{depth} \times \text{fan-out})$ time per $\pm 1$ leaf perturbation.
- **`FeasibilityTippingPoint`**:
  - `step() -> Option<NodeIdx>`, `node() -> Option<NodeId>`, `from_step_feasibility() -> FeasibilityLevel`, `to_step_feasibility() -> FeasibilityLevel`, `from_threat_feasibility() -> FeasibilityLevel`, `to_threat_feasibility() -> FeasibilityLevel`, `from_risk() -> RiskLevel`, `to_risk() -> RiskLevel`.
- **`ImpactTippingPoint`**:
  - `damage() -> DamageId`, `category() -> ImpactCategory`, `from_impact() -> Level`, `to_impact() -> Level`, `from_risk() -> RiskLevel`, `to_risk() -> RiskLevel`.
  - Uses precomputed `(max1, max1_count, max2)` across rated `(damage, category)` entries so each $\pm 1$ impact perturbation computes `to_risk` in $O(1)$ time and handles tied deciding entries accurately.
- **`ThreatSensitivity`**:
  - `feasibility() -> FeasibilityLevel`, `max_risk() -> Option<RiskLevel>`, `feasibility_tipping_points() -> &[FeasibilityTippingPoint]`, `impact_tipping_points() -> &[ImpactTippingPoint]`, `is_stable() -> bool`.
- **`Project` Methods**:
  - `tree_choke_points`, `residual_tree_choke_points`, `threat_choke_points`, `residual_threat_choke_points`
  - `tree_hitting_set`, `residual_tree_hitting_set`, `threat_hitting_set`, `residual_threat_hitting_set`
  - `tree_tipping_points`, `residual_tree_tipping_points`, `threat_sensitivity`, `residual_threat_sensitivity`

### 4. Verification
- Unit tests in `paths.rs`, integration tests in `tests/project.rs` (including rule R6 `SENTINEL` verification), and property tests in `tests/eval_properties.rs` and `tests/ops_properties.rs` against independent naive perturbation and path-counting oracles.
