# 0021. Multi-factor attack potential feasibility

Date: 2026-04-21
Status: Accepted

## Context

In ISO/SAE 21434 Clause 15.6 and Annex G, attack feasibility can be assessed using either a coarse concept-phase rating (such as an attack-vector or direct feasibility rating) or a detailed **attack potential** approach derived from the Common Criteria Evaluation Methodology (CEM, ISO/IEC 18045). Under the attack potential approach, an analyst rates each attack step across multiple methodology-defined factors (in CEM: elapsed time, expertise, knowledge of the item, window of opportunity, and equipment), each factor choice contributes a point value, the point values are summed, and the total score maps through contiguous bands to a `FeasibilityLevel` (higher attack potential required by the attacker means lower attack feasibility).

Supporting multi-factor attack potential in `aegis-model` requires deciding:

1. How factor vocabularies, point values, and score-to-feasibility bands are declared in `MethodologyDef` and methodology pack files without hardcoding automotive or CEM constants in library code (rule A2).
2. How per-step factor ratings are stored, mutated, and explained (`AttackPotential` breakdown) on `Project` attack trees while keeping unrated steps allocation-free.
3. How multi-factor ratings interact with direct `step_feasibility` ratings across inherent evaluation, residual evaluation, top-$k$ path enumeration, project linting, and project diffing.
4. How `step_factors` and `Op::SetStepFactor` are persisted in snapshots and op logs without altering existing v1 golden fixtures.

## Decision

1. **Methodology-defined `AttackPotentialDef` (`methodology/mod.rs`, `methodology/pack.rs`):**
   - `MethodologyDef` carries `attack_potential: Option<AttackPotentialDef>` (omitted in pack JSON when `None`).
   - `AttackPotentialDef` declares:
     - `factors: Vec<FactorDef>` (`1..=16` factors, `MIN_FEASIBILITY_FACTORS..=MAX_FEASIBILITY_FACTORS`), where each `FactorDef` has a unique key, label, and `2..=16` `FactorLevelDef` choices (`key`, `label`, `points: u16` capped at `MAX_FACTOR_POINTS = 1_000`).
     - `bands: Vec<PotentialBandDef>` (`1..=16` bands), where each band specifies `min: u16`, `max: Option<u16>`, and `feasibility: Arc<str>` (referencing `feasibility_levels`).
   - `Methodology::new` validates that `bands` form a contiguous, non-overlapping partition of `0..`: band 0 starts at `min == 0`, every band except the last has `max == Some(m)` with `m >= min` and `next.min == m + 1`, and the last band has `max == None` (`>= min`). Consequently, every possible sum of factor points maps deterministically to a single `PotentialBand` and `FeasibilityLevel` with no unmapped score gaps.
   - `FeasibilityFactor` and `FactorLevel` are 1-byte `vocab_index!` types handed out by `Methodology`.
   - `packs/iso21434-r155.json` declares the five public CEM (ISO/IEC 18045) factors (`elapsed-time`, `expertise`, `knowledge`, `window-of-opportunity`, `equipment`) and four score bands (`0..=13 -> high`, `14..=19 -> medium`, `20..=24 -> low`, `>= 25 -> very-low`).

2. **Per-step factor storage, `Op::SetStepFactor`, and `AttackPotential` breakdown:**
   - `TreeEntry` stores `step_factors: Vec<Box<[Option<FactorLevel>]>>` indexed by `NodeIdx::index()`. Steps with no factor ratings hold an empty slice `Box::new([])` (zero heap allocation); when at least one factor is rated, the slice holds one slot per declared `FeasibilityFactor`, and shrinks back to `Box::new([])` if all factors on the step are cleared.
   - `Op::SetStepFactor { tree, node, factor, level, provenance }` sets, updates, or clears (`level: None`) one factor rating on a leaf step, updates the tree's provenance, and returns the exact prior `Op::SetStepFactor` as its single-op inverse in $O(1)$ time.
   - `Op::RemoveAttackTree` requires a tree to have zero factor ratings across its steps (`ModelError::AttackTreeHasFactors { tree, factors }`), preserving `Op::AddAttackTree` as its exact single-op inverse.
   - When every declared `FeasibilityFactor` is rated on `(tree, node)`, `Project::step_attack_potential(tree, node)` (via `Methodology::resolve_attack_potential`) returns `Some(AttackPotential)` containing:
     - `factors: Box<[FactorContribution]>` (`(factor, level, points)` in declaration order),
     - `score: u32` (sum of `points`),
     - `band: PotentialBand` (`min`, `max`, `feasibility`), and
     - `feasibility: FeasibilityLevel`.

3. **Precedence, evaluation, lint, and diff:**
   - `Project::effective_step_feasibility(tree, node)` returns `Some(ap.feasibility())` when all declared factors are rated on `(tree, node)` (`step_attack_potential` is `Some`), and otherwise falls back to `self.step_feasibility(tree, node)`.
   - Just as a linked `AttackTree` overrides `ThreatScenario::direct_feasibility` without erasing it (ADR 0016), complete `step_factors` override a step's coarse `step_feasibility` without erasing it; clearing a factor falls back cleanly to `step_feasibility`.
   - `residual_step_feasibility`, `evaluate_tree`, `evaluate_threat`, `evaluate_risk`, `top_k_paths`, `evaluate_residual_tree`, `evaluate_residual_threat`, `evaluate_residual_risk`, and `top_k_residual_paths` all use `effective_step_feasibility`.
   - `Project::lint` emits:
     - `LintCode::AttackTreeStepUnrated { tree, node }` (`Error`) when `effective_step_feasibility(tree, node)` is `None`, preserving the lint soundness guarantee.
     - `LintCode::AttackTreeStepIncompleteFactors { tree, node }` (`Warning`) when a step has partial factor ratings (`0 < rated < factor_count`).
     - `LintCode::RedundantStepFeasibility { tree, node }` (`Warning`) when a step has both complete factor ratings and a direct `step_feasibility` rating.
   - `Project::diff` compares `step_factors` inside `Project::same_tree`, propagating factor edits to affected threat risk deltas.

4. **Snapshot and op-log persistence:**
   - In `aegis-project` snapshots, `TreeView` serializes `step_factors: { "<node_id>": { "<factor_key>": "<factor_level_key>" } }` (nodes in arena order, factors in ascending factor-key order) and omits `step_factors` when empty.
   - In `aegis-oplog`, `Op::SetStepFactor` encodes as `{"op":"set_step_factor","tree":"...","node":"...","factor":"...","level":"..."|null,"provenance":...}`.
   - Decode-time caps enforce `Limit::FeasibilityFactors { max: 16 }` (`Limit::FEASIBILITY_FACTORS`) on factor lists and maps before model validation runs.

## Consequences

- Analysts can rate attack steps with either coarse `FeasibilityLevel` ratings or detailed multi-factor attack potential ratings, and inspect the exact factor-by-factor point sum and matched score band via `Project::step_attack_potential`.
- All factor names, level choices, point values, and score band thresholds live purely in methodology packs (rule A2).
- Unrated steps incur zero extra heap allocations, keeping all 19 `benches/store_memory.rs` gates within budget, and all v1 golden fixtures remain byte-for-byte compatible.
