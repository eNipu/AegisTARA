# 023 — Multi-factor attack potential feasibility in `aegis-model`

## Goal

Add a methodology-defined multi-factor attack potential scheme (`AttackPotentialDef`, `FactorDef`, `FactorLevelDef`, `PotentialBandDef`) and per-step factor ratings (`Op::SetStepFactor`, `AttackPotential`, `FactorContribution`, `PotentialBand`) so analysts can rate individual factors (such as the five Common Criteria / ISO 18045 factors in ISO/SAE 21434) on attack tree steps, automatically derive the step's inherent `FeasibilityLevel` with a full factor-by-factor point breakdown, and evaluate inherent and residual tree feasibility and threat risk seamlessly.

## Scope

1. **Methodology vocabulary & pack format (`methodology/mod.rs`, `methodology/pack.rs`, `packs/iso21434-r155.json`)**:
   - Constants: `MIN_FEASIBILITY_FACTORS: usize = 1`, `MAX_FEASIBILITY_FACTORS: usize = 16`, `MAX_FACTOR_POINTS: u16 = 1_000`.
   - Definition types: `AttackPotentialDef`, `FactorDef`, `FactorLevelDef`, `PotentialBandDef`, and `MethodologyDef::attack_potential: Option<AttackPotentialDef>` (omitted in canonical pack JSON when `None`).
   - Validated vocabulary indices: `FeasibilityFactor` and `FactorLevel` via `vocab_index!`, plus `PotentialBand`, `FactorContribution`, and `AttackPotential`.
   - Built-in `packs/iso21434-r155.json` (and `fuzz/seeds/pack/iso21434-r155.json`) updated with the five Common Criteria (ISO/IEC 18045) attack potential factors (`elapsed-time`, `expertise`, `knowledge`, `window-of-opportunity`, `equipment`) and four score bands (`0..=13 -> high`, `14..=19 -> medium`, `20..=24 -> low`, `>= 25 -> very-low`).
   - Second methodology fixture (`med-device-demo.json` / `second_def()`) updated with a 3-factor medical probability scheme (`exposure`, `skill`, `resources`) and 5 bands to prove methodology neutrality (rule A2).

2. **Errors, vocabulary kinds, and decode caps (`error.rs`, `store/mod.rs`)**:
   - `MethodologyError` variants: `FeasibilityFactorCount`, `DuplicateFeasibilityFactor`, `FactorLevelCount`, `DuplicateFactorLevel`, `FactorPointsTooLarge`, `PotentialBandCount`, `PotentialBandRange`, `UnknownBandFeasibilityLevel`.
   - `ModelError` variants: `UnknownFeasibilityFactor`, `UnknownFactorLevel`, `AttackTreeHasFactors`.
   - `VocabKind` variants: `FeasibilityFactor`, `FactorLevel`.
   - `Limit::FeasibilityFactors { max: usize }` (`Limit::FEASIBILITY_FACTORS`).

3. **Operations, evaluation, lint, and diff (`op.rs`, `project.rs`, `lint.rs`, `lib.rs`)**:
   - `Op::SetStepFactor { tree, node, factor, level, provenance }` with $O(1)$ validation and exact single-op inverse.
   - `Project` accessors: `step_factor`, `step_factors`, `step_attack_potential`, `effective_step_feasibility`.
   - Precedence: complete step factor ratings (`step_attack_potential(tree, node)`) derive the step's inherent `FeasibilityLevel` and override coarse `step_feasibility(tree, node)` without erasing it; `evaluate_tree`, `evaluate_residual_tree`, `evaluate_risk`, `evaluate_residual_risk`, `top_k_paths`, and `top_k_residual_paths` all use `effective_step_feasibility`.
   - `Project::lint` checks: `LintCode::AttackTreeStepIncompleteFactors { tree, node }` (when `0 < rated_factors < factor_count`) and `LintCode::RedundantStepFeasibility { tree, node }` (when both complete factor ratings and direct `step_feasibility` are set on a step).
   - `Project::diff` compares `step_factors` in `same_tree` so factor edits propagate to affected threat risk deltas.

4. **Persistence (`store/{wire,snapshot,op_log}.rs`)**:
   - Snapshot `TreeView` / `TreeWire` encodes `step_factors: { "<node_id>": { "<factor_key>": "<factor_level_key>" } }` sorted by factor key and omitted when empty, keeping `tests/fixtures/project-v1.json` and `tests/fixtures/oplog-v1.jsonl` byte-for-byte identical.
   - Op-log `OpView` / `OpWire` encodes and decodes `set_step_factor`.

5. **Tests and ADR**:
   - Unit, integration, property, pack, and store limit tests, plus `docs/adr/0021-attack-potential-feasibility.md`.
