VERDICT: PASS

# Code Review — `023-attack-potential-feasibility` (Round 1)

## Summary

The implementation of methodology-driven multi-factor attack potential feasibility (`023-attack-potential-feasibility`) satisfies all 12 acceptance criteria (AC1–AC12), Rust rules (R1–R16), architecture rules (A1–A9), and quality standards in `docs/engineering-standards.md`.

Verification gates executed and passed:
- `./scripts/ci.sh` (`cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`): 175 unit tests, 63 integration tests, and 79 doctests pass with zero warnings.
- `./scripts/deny.sh`: `aegis-model` and `aegis-model/fuzz` pass with zero new dependencies added to `Cargo.toml`.
- `cargo test --locked --benches` in `crates/aegis-model`: all benchmark and `dhat` heap memory gates (`store_memory`) pass (`attack_trees` kept factor `2.35x <= 2.4x <= 3.0x` server budget).

---

## Acceptance Criteria Verification (AC1–AC12)

- **AC1 (Methodology Attack Potential Schema & Validation)**: **PASS**
  - `MethodologyDef` adds `pub attack_potential: Option<AttackPotentialDef>` (`crates/aegis-model/src/methodology/mod.rs:146`).
  - `AttackPotentialDef`, `FactorDef`, `FactorLevelDef`, and `PotentialBandDef` are defined and documented with runnable doctests (`crates/aegis-model/src/methodology/mod.rs:304-380`).
  - `Methodology::build_attack_potential` (`crates/aegis-model/src/methodology/mod.rs:886-982`) enforces:
    - `1..=16` factors (`MIN_FEASIBILITY_FACTORS..=MAX_FEASIBILITY_FACTORS`), unique valid keys, non-blank labels (`<= 256` chars), else `MethodologyError::FeasibilityFactorCount` / `DuplicateFeasibilityFactor`.
    - `2..=16` levels per factor (`MIN_LEVELS..=MAX_LEVELS`), unique valid keys within the factor, non-blank labels, `points <= 1_000` (`MAX_FACTOR_POINTS`), else `MethodologyError::FactorLevelCount` / `DuplicateFactorLevel` / `FactorPointsTooLarge`.
    - `1..=16` bands (`1..=MAX_LEVELS`), contiguous non-overlapping coverage of `0..` (`band[0].min == 0`, non-final `band[i]` has `max == Some(m)` with `m >= min` and `band[i + 1].min == m + 1`, final band has `max == None`), else `MethodologyError::PotentialBandCount` / `PotentialBandRange`.
    - Every band `feasibility` resolves in `feasibility_levels`, else `MethodologyError::UnknownBandFeasibilityLevel` with key truncated via `error_key_text` (`<= 64` bytes).
  - Pack wire serialization (`crates/aegis-model/src/methodology/pack.rs:157-212`, `362-454`) uses `#[serde(default)]` on `attack_potential`, omits `"attack_potential"` when `None`, and omits `"max"` on the final band when `None`.
  - Pack decode caps (`crates/aegis-model/src/methodology/pack.rs:62-66`, `172-209`) enforce `Limit::FeasibilityFactors { max: 16 }`, `Limit::Levels { max: 16 }`, `Limit::KeyBytes { max: 64 }`, and `Limit::LabelBytes { max: 1024 }`, verified in `crates/aegis-model/tests/pack_limits.rs:99-125`, `170-188`, `262-310`.

- **AC2 (Compiled Vocabulary Handles, Band Resolution & Explain-Why Value)**: **PASS**
  - `FeasibilityFactor(u8)` and `FactorLevel(u8)` (`crates/aegis-model/src/methodology/mod.rs:432-433`) are `Copy`, 1-byte (`size_of == 1`, verified at `crates/aegis-model/src/methodology/mod.rs:1935-1936`) handles with `index(self) -> usize`, `Ord`, and `Hash`.
  - `PotentialBand`, `FactorContribution`, and `AttackPotential` (`crates/aegis-model/src/methodology/mod.rs:461-624`) provide all specified accessors.
  - `Methodology` (`crates/aegis-model/src/methodology/mod.rs:1244-1432`) provides `has_attack_potential`, `feasibility_factor_count`, `feasibility_factors`, `feasibility_factor`, `feasibility_factor_key`, `feasibility_factor_label`, `factor_level_count`, `factor_levels`, `factor_level`, `factor_level_key`, `factor_level_label`, `factor_level_points`, `potential_bands`, `resolve_attack_potential`, and zero-allocation `resolve_attack_potential_feasibility`.

- **AC3 (Built-In Pack & Second Methodology Pack)**: **PASS**
  - `crates/aegis-model/packs/iso21434-r155.json:242-401` and `crates/aegis-model/fuzz/seeds/pack/iso21434-r155.json` include the ISO/SAE 21434:2021 Annex G.3 / Tables G.1–G.7 5-factor scheme (`elapsed-time`, `expertise`, `knowledge`, `window-of-opportunity`, `equipment`) and 4 potential bands (`0..=13 -> high`, `14..=19 -> medium`, `20..=24 -> low`, `25.. -> very-low`).
  - `crates/aegis-model/tests/fixtures/packs/med-device-demo.json:180-277`, `crates/aegis-model/fuzz/seeds/pack/med-device-demo.json`, and `crates/aegis-model/tests/common/mod.rs:232-330` include the 3-factor medical-device scheme (`exposure`, `skill`, `resources`) and 5 bands (`0..=2 -> frequent`, `3..=5 -> probable`, `6..=8 -> occasional`, `9..=11 -> remote`, `12.. -> improbable`).
  - Both packs are canonical `write_pack` output (`crates/aegis-model/tests/builtin_pack.rs:344-352`, `crates/aegis-model/tests/second_methodology.rs:39-74`).

- **AC4 (Reversible Step Factor Mutation & Removal Guard)**: **PASS**
  - `Op::SetStepFactor` (`crates/aegis-model/src/op.rs:260-273`) is reversible: `Project::apply` (`crates/aegis-model/src/project.rs:1726-1738`, `2056-2092`) validates `UnknownAttackTree`, `UnknownTreeNode`, `NotAStep`, `UnknownFeasibilityFactor`, `UnknownFactorLevel`, and `check_provenance`, then returns the inverse `Op::SetStepFactor` with the previous `Option<FactorLevel>` and `TreeEntry::provenance`.
  - Sparse storage: `TreeEntry::step_factors` (`crates/aegis-model/src/project.rs:97`) uses `Box<[Option<FactorLevel>]>` per node, keeping `Box::default()` (`len == 0`) when unrated and shrinking back to `Box::default()` when all factors on a step are cleared (`crates/aegis-model/src/project.rs:2084-2086`), enforced by `tree_invariants` (`crates/aegis-model/src/project.rs:2199-2216`).
  - `Op::RemoveAttackTree` (`crates/aegis-model/src/project.rs:1770-1778`) rejects removal with `ModelError::AttackTreeHasFactors(id)` after `AttackTreeInUse` and before `AttackTreeHasMitigations`.

- **AC5 (Precedence & Evaluation Integration)**: **PASS**
  - `Project::step_factor`, `step_factors`, `step_attack_potential`, and `effective_step_feasibility` (`crates/aegis-model/src/project.rs:582-691`) implement the exact precedence: explicit `step_feasibility` override wins; otherwise all-factors-rated derives feasibility via `resolve_attack_potential_feasibility`; otherwise `None`.
  - `residual_step_feasibility` (`crates/aegis-model/src/project.rs:733-746`) uses `effective_step_feasibility` as the inherent baseline before taking `min` with `step_mitigations`.
  - `evaluate_tree`, `evaluate_residual_tree`, `evaluate_threat`, `evaluate_residual_threat`, `evaluate_risk`, `evaluate_residual_risk`, `top_k_paths`, `top_k_residual_paths`, `top_k_threat_paths`, and `top_k_residual_threat_paths` all route through `effective_step_feasibility` and `residual_step_feasibility` (`crates/aegis-model/src/project.rs:774-869`).

- **AC6 (Project Linter Integration)**: **PASS**
  - `LintCode` (`crates/aegis-model/src/lint.rs:124-142`) adds `AttackTreeStepIncompleteFactors { tree, node, rated, required }` and `RedundantStepFeasibility { tree, node }` with `LintSeverity::Warning` (`crates/aegis-model/src/lint.rs:169-170`) and clear `Display` messages (`crates/aegis-model/src/lint.rs:265-280`).
  - `size_of::<LintCode>() <= 40` is asserted at `crates/aegis-model/src/lint.rs:321`.
  - `Project::lint_trees` (`crates/aegis-model/src/project.rs:1066-1126`) suppresses `AttackTreeUnratedStep` when all factors are rated, emits `AttackTreeStepIncompleteFactors` when `0 < rated < F` and no explicit `step_feasibility` is set, and emits `RedundantStepFeasibility` when both explicit `step_feasibility` and `rated == F` are present.

- **AC7 (Project Equality & Diff Integration)**: **PASS**
  - `Project::same_tree` (`crates/aegis-model/src/project.rs:1203-1228`), shared by `PartialEq for Project` and `Project::diff`, compares `step_factor(a_idx, node_idx, factor) == other.step_factor(b_idx, node_idx, factor)` for every step and factor, marking trees as modified in `diff.attack_trees().modified()` and propagating derived feasibility changes into `diff.risk_deltas()`.

- **AC8 (Snapshot & Op-Log Wire Persistence)**: **PASS**
  - Snapshot `TreeView` (`crates/aegis-model/src/store/snapshot.rs:75-86`) serializes `step_factors` with `#[serde(skip_serializing_if = "StepFactorsByNode::is_empty")]`, ordered by arena node order and ascending factor key order (`KeyOrder::feasibility_factors`, `crates/aegis-model/src/store/wire.rs:1585-1586`, `1894-1956`).
  - Snapshot `TreeWire` (`crates/aegis-model/src/store/wire.rs:802-912`, `1306-1316`) decodes `step_factors` with `#[serde(default)]`, rejecting empty outer/inner maps, duplicate nodes/factors, unknown factor/level keys (`VocabKind::FeasibilityFactor` / `VocabKind::FactorLevel`), and over-cap inputs (`Limit::FeasibilityFactors { max: 16 }`, `Limit::AttackTreeNodes { max: 32 }`).
  - Op-log `Op::SetStepFactor` (`crates/aegis-model/src/store/op_log.rs:285-305`, `537-552`, `690-698`; `crates/aegis-model/src/store/wire.rs:1466-1474`, `1538-1548`) encodes/decodes `{"op":"set_step_factor","tree":...,"node":...,"factor":...,"level":...}` with `within_caps` validation.
  - Frozen golden fixtures `crates/aegis-model/tests/fixtures/project-v1.json` and `crates/aegis-model/tests/fixtures/oplog-v1.jsonl` are untouched in `git status`.

- **AC9 (No Customer Names in Errors)**: **PASS**
  - All new `MethodologyError` and `ModelError` variants (`crates/aegis-model/src/error.rs:132-193`, `356-399`, `449-453`) carry only indices, counts, points, `TreeId`, `NodeId`, `FeasibilityFactor`, `FactorLevel`, or bounded key text (`error_key_text`, `<= 64` bytes). Verified by `methodology_error_key_text_is_bounded_in_every_position` (`crates/aegis-model/src/methodology/mod.rs:1985-2046`) and `errors_have_no_labels` (`crates/aegis-model/tests/store.rs:1224-1327`).

- **AC10 (Determinism)**: **PASS**
  - Verified by `save_is_deterministic` (`crates/aegis-model/tests/store.rs:245-445`) and `key_order_sorts_by_key` (`crates/aegis-model/src/store/wire.rs:2380-2403`). No `HashMap` or `HashSet` is used anywhere in the crate.

- **AC11 (Methodology Independence)**: **PASS**
  - Verified in `crates/aegis-model/tests/second_methodology.rs:39-315` and across all property tests (`ops_second_methodology`, `lint_soundness_second_methodology`, `diff_soundness_second_methodology` in `crates/aegis-model/tests/ops_properties.rs`).

- **AC12 (Proptest Invariants & Op Coverage)**: **PASS**
  - `Gen` (`crates/aegis-model/tests/common/mod.rs:529-546`, `985-1039`) generates valid and invalid `Op::SetStepFactor` operations and exercises `AttackTreeHasFactors` on `RemoveAttackTree`.
  - `ops_generator_covers_every_op_and_most_errors` (`crates/aegis-model/tests/ops_properties.rs:229-244`) asserts `stats.ok_kinds.len() == 27` and `stats.errors.len() >= 30`.
  - `Project::check_invariants` (`crates/aegis-model/src/project.rs:2199-2216`) verifies `step_factors` length, canonical sparse storage, and factor-level bounds after every op.

---

## Rust Rules (R1–R16) & Architecture Rules (A1–A9)

- **R1 (`#![forbid(unsafe_code)]`)**: Present in `crates/aegis-model/src/lib.rs:33` and `crates/aegis-model/benches/store_memory.rs:78`.
- **R2 (No `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!`/` unreachable!` in library code)**: Verified across all non-test modules in `crates/aegis-model/src/`.
- **R3 (Typed error enums implementing `Error + Display + Send + Sync + 'static`)**: Verified in `crates/aegis-model/src/error.rs` and `crates/aegis-model/src/store/mod.rs`.
- **R4 (Bounded types & arenas)**: `FeasibilityFactor(u8)`, `FactorLevel(u8)`, `MAX_FEASIBILITY_FACTORS = 16`, `MAX_FACTOR_POINTS = 1_000`.
- **R5 (Small value types `Copy`)**: `FeasibilityFactor`, `FactorLevel`, `PotentialBand`, `FactorContribution`, `LintCode` are all `Copy`.
- **R6 (Deterministic collections)**: `BTreeSet` and sorted slices (`binary_search_by`) used throughout; zero `HashMap`/`HashSet`.
- **R7 (`#[must_use]` on pure functions/accessors)**: Applied on all new public types and methods in `methodology/mod.rs`, `lint.rs`, and `project.rs`.
- **R8–R16 (Clippy pedantic, rustdoc examples, module size, no globals/locks, zero new dependencies)**: All clippy pedantic lints pass with `-D warnings`; all public types and methods have rustdoc with runnable examples and `# Errors` sections where fallible.
- **A1–A9 (Core vs. pack boundary, reversible ops, explain-why traces, provenance, bounded decode caps)**: Fully respected.

## Blocking Issues

None.

## Non-Blocking Notes

None.

VERDICT: PASS
