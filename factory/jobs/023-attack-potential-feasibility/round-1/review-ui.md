VERDICT: PASS

No UI change (no `.slint` or GUI files touched in job `023-attack-potential-feasibility`).

### UI-Readiness Verification (`crates/aegis-model`)

1. **`AttackPotential`, `FactorContribution`, and `PotentialBand` (`crates/aegis-model/src/methodology/mod.rs:547-829`, `2305-2598`; `crates/aegis-model/src/project.rs:1096-1247`)**:
   - `PotentialBand` (`Copy`, `Eq`) exposes `min() -> u16`, `max() -> Option<u16>`, `feasibility() -> FeasibilityLevel`, and `contains(score: u32) -> bool` in O(1), providing exact bounds (`min..=max` or `min..` for the open-ended top band) and the derived `FeasibilityLevel` for score-band badges and table headers.
   - `FactorContribution` (`Copy`, `Eq`) exposes `factor() -> FeasibilityFactor`, `level() -> FactorLevel`, and `points() -> u16` in O(1), allowing step inspector panels to render each rated factor alongside its chosen level and point contribution without secondary lookups.
   - `AttackPotential` (`Clone`, `Eq`) exposes `factors() -> &[FactorContribution]` in methodology declaration order, `score() -> u32`, `band() -> PotentialBand`, and `feasibility() -> FeasibilityLevel` in O(1), giving the UI the full explainability breakdown (`factor -> level -> points -> sum -> band -> feasibility`) in one value.
   - `Methodology` exposes full introspection for dynamic UI form generation without hard-coding ISO 21434 factor names: `feasibility_factors()`, `feasibility_factor_count()`, `feasibility_factor(key)`, `feasibility_factor_key(f)`, `feasibility_factor_label(f)`, `factor_levels(f)`, `factor_level_count(f)`, `factor_level(f, key)`, `factor_level_key(f, l)`, `factor_level_label(f, l)`, `factor_level_points(f, l)`, `potential_bands()`, `potential_band_for_score(score)`, and `resolve_attack_potential(ratings)`.
   - `Project` exposes `step_factor(tree, node, factor)`, `step_factors(tree, node)` (in methodology factor declaration order), `step_attack_potential(tree, node)`, and `effective_step_feasibility(tree, node)` (preferring complete factor-derived feasibility over direct step feasibility), supporting both complete and partial/in-progress factor editing in the UI.

2. **`LintCode` Display & Severity (`crates/aegis-model/src/lint.rs:149-165`, `210-228`, `253-270`)**:
   - `LintCode::AttackTreeStepIncompleteFactors { tree, node }` (`LintSeverity::Warning`) formats as:
     `"attack tree {tree} step {node} has incomplete attack potential factor ratings"`
   - `LintCode::RedundantStepFeasibility { tree, node }` (`LintSeverity::Warning`) formats as:
     `"attack tree {tree} step {node} has a direct feasibility rating shadowed by its attack potential factors"`
   - Both messages are clear, actionable for UI lint panels, and strictly adhere to rule R6 (identifying entities by stable `TreeId`/`NodeId` only, never user-supplied labels). `LintFinding` remains `Copy` and `<= 40` bytes.

3. **Error Display & Wire Representation (`crates/aegis-model/src/error.rs:97-285`, `390-613`; `crates/aegis-model/src/store/mod.rs:359-451`; `crates/aegis-model/src/store/wire.rs:163-199`, `551-656`)**:
   - All 8 new `MethodologyError` variants (`FeasibilityFactorCount`, `DuplicateFeasibilityFactor`, `FactorLevelCount`, `DuplicateFactorLevel`, `FactorPointsTooLarge`, `PotentialBandCount`, `PotentialBandRange`, `UnknownBandFeasibilityLevel`) and 3 new `ModelError` variants (`UnknownFeasibilityFactor`, `UnknownFactorLevel`, `AttackTreeHasFactors`) have clear, PII-safe `Display` implementations.
   - `VocabKind::FeasibilityFactor` (`"feasibility factor"`), `VocabKind::FactorLevel` (`"factor level"`), and `Limit::FeasibilityFactors { max }` (`"more than {max} feasibility factors"`) format cleanly in `StoreError` messages.
   - Snapshot JSON (`step_factors`: `{ "<node_id>": { "<factor_key>": "<factor_level_key>" } }` sorted by factor key, omitted when empty) and op-log JSON (`set_step_factor` with `level: Option<String>`) use stable string keys rather than vocabulary indices and round-trip deterministically.
   - Built-in pack `crates/aegis-model/packs/iso21434-r155.json` and fixture `crates/aegis-model/tests/fixtures/packs/med-device-demo.json` provide clean, well-capitalized human labels (`"Elapsed time"`, `"<= 1 day"`, `"Knowledge of the item"`, `"Window of opportunity"`, `"Equipment"`, etc.) ready for direct display in UI controls.

4. **Rustdoc Formatting**:
   - All new public types (`FeasibilityFactor`, `FactorLevel`, `PotentialBand`, `FactorContribution`, `AttackPotential`, `AttackPotentialDef`, `FactorDef`, `FactorLevelDef`, `PotentialBandDef`), constants (`MIN_FEASIBILITY_FACTORS`, `MAX_FEASIBILITY_FACTORS`, `MAX_FACTOR_POINTS`), enum variants, and methods have complete rustdoc comments, complexity annotations, and runnable doctests.

VERDICT: PASS
