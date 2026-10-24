# Build Log: `023-attack-potential-feasibility`

## Summary of Changes

1. **Methodology Vocabulary & Pack Format (`crates/aegis-model/src/methodology/mod.rs`, `crates/aegis-model/src/methodology/pack.rs`, `crates/aegis-model/packs/iso21434-r155.json`, `crates/aegis-model/tests/fixtures/packs/med-device-demo.json`)**:
   - Added `AttackPotentialDef`, `FactorDef`, `FactorLevelDef`, `PotentialBandDef`, `FeasibilityFactor`, `FactorLevel`, `PotentialBand`, `FactorContribution`, and `AttackPotential`, with `MethodologyDef::attack_potential: Option<AttackPotentialDef>` (optional in JSON packs via `#[serde(default)]` and omitted when `None`).
   - Added `MIN_FEASIBILITY_FACTORS` (`1`), `MAX_FEASIBILITY_FACTORS` (`16`), and `MAX_FACTOR_POINTS` (`1_000`) validation rules and contiguous non-overlapping score-band validation in `Methodology::new`.
   - Implemented `Methodology::resolve_attack_potential` and zero-allocation `Methodology::resolve_attack_potential_feasibility`.
   - Declared the 5 CEM factors and 4 score bands in `packs/iso21434-r155.json`, and 3 heterogeneous factors (`[3, 4, 3]` levels) and 5 score bands in `med-device-demo.json`.
2. **Errors, VocabKind, and Decode Caps (`crates/aegis-model/src/error.rs`, `crates/aegis-model/src/store/mod.rs`, `crates/aegis-model/src/store/wire.rs`)**:
   - Added `MethodologyError` variants (`FeasibilityFactorCount`, `DuplicateFeasibilityFactor`, `FactorLevelCount`, `DuplicateFactorLevel`, `FactorPointsTooLarge`, `PotentialBandCount`, `PotentialBandRange`, `UnknownBandFeasibilityLevel`).
   - Added `ModelError` variants (`UnknownFeasibilityFactor`, `UnknownFactorLevel`, `AttackTreeHasFactors`).
   - Added `VocabKind::FeasibilityFactor`, `VocabKind::FactorLevel`, and `Limit::FeasibilityFactors { max: usize }` (`Limit::FEASIBILITY_FACTORS`).
3. **Operations, Project State, Evaluation, Lint, and Diff (`crates/aegis-model/src/op.rs`, `crates/aegis-model/src/project.rs`, `crates/aegis-model/src/lint.rs`)**:
   - Added `Op::SetStepFactor { tree, node, factor, level, provenance }` with canonical sparse `TreeEntry::step_factors` storage (`Box<[]>` when no factors are rated on a step).
   - Added `Project::step_factor`, `Project::step_factors`, `Project::step_attack_potential`, and `Project::effective_step_feasibility`.
   - Updated tree/threat/risk/path evaluation and `Project::diff` to use `effective_step_feasibility`.
   - Added `LintCode::AttackTreeStepIncompleteFactors` and `LintCode::RedundantStepFeasibility` to `Project::lint`.
4. **Store Persistence (`crates/aegis-model/src/store/wire.rs`, `crates/aegis-model/src/store/snapshot.rs`, `crates/aegis-model/src/store/op_log.rs`)**:
   - Added canonical `step_factors` snapshot serialization/deserialization (omitted when empty, sorted by node order then factor key) and `set_step_factor` op-log serialization/deserialization.
5. **Tests & Documentation**:
   - Added `docs/adr/0021-attack-potential-feasibility.md`.
   - Extended unit tests, pack/store cap tests, second-methodology tests, and proptest generators (`27` `Op` variants covered).

## Verification

- `./scripts/ci.sh` passed (`fmt`, `clippy`, `test`, `doc` — 175 unit tests, 14 integration test suites, 411 doctests).
- `./scripts/deny.sh` passed.
- `cargo test --benches --manifest-path crates/aegis-model/Cargo.toml` passed all memory and timing benchmark gates.
