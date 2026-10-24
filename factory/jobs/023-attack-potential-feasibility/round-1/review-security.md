VERDICT: PASS

# Security Review — `023-attack-potential-feasibility` (Round 1)

## Checklist Evaluation (S1–S11)

- **S1 (`unsafe` blocks — Q2)**: **PASS**. `crates/aegis-model/src/lib.rs:217` enforces `#![forbid(unsafe_code)]` at the crate root. No `unsafe` blocks, attributes, or raw pointer operations exist anywhere in the modified or added code.
- **S2 (`unwrap` / `expect` / `panic!` / indexing in library code — Q3)**: **PASS**. Library code contains zero `.unwrap()`, `.expect()`, `panic!`, `todo!`, or `unimplemented!` calls. All slice accesses on dynamic indices use checked `.get()` / `.get_mut()` (`crates/aegis-model/src/methodology/mod.rs:2513-2532`, `crates/aegis-model/src/project.rs:1463-1556`, `crates/aegis-model/src/project.rs:4127-4145`). The only `.expect()` calls in `crates/aegis-model/src/` are inside `#[cfg(test)]` modules.
- **S3 (Capped deserialization before allocation — R3, R6, Q4)**: **PASS**.
  - Methodology pack wire decoding (`crates/aegis-model/src/methodology/pack.rs:58-63`, `146-197`) bounds `factors` with `BoundedVec<Object<FactorWire>, FeasibilityFactorsCap>` (`MAX_FEASIBILITY_FACTORS = 16`, `Limit::FEASIBILITY_FACTORS`), factor `levels` with `BoundedVec<Object<FactorLevelWire>, LevelsCap>` (`MAX_LEVELS = 16`, `Limit::LEVELS`), and `bands` with `BoundedVec<Object<PotentialBandWire>, LevelsCap>`. All wire structs (`AttackPotentialWire`, `FactorWire`, `FactorLevelWire`, `PotentialBandWire`) carry `#[serde(deny_unknown_fields)]` and wrap keys/labels in `Key` (`BoundedStr<KeyCap>`, 64 bytes) and `Label` (`BoundedStr<LabelCap>`, 256 bytes).
  - Snapshot `StepFactors` and `NodeFactors` streaming visitors (`crates/aegis-model/src/store/wire.rs:886-992`) enforce entry caps during `MapAccess::next_entry_seed` before pushing: `StepFactors` caps outer entries at `MAX_ATTACK_TREE_NODES` (`Limit::ATTACK_TREE_NODES`) and `NodeFactors` caps per-step entries at `MAX_FEASIBILITY_FACTORS` (`Limit::FEASIBILITY_FACTORS`). Key lengths are checked via `check_key_len` (`MAX_KEY_BYTES = 64`) before vocabulary lookup (`crates/aegis-model/src/store/wire.rs:845`, `863`, `919`).
  - Whole-file and per-record byte caps (`CappedReader` with `MAX_PACK_FILE_BYTES`, `MAX_PROJECT_FILE_BYTES`, `MAX_OP_RECORD_BYTES`) remain intact on all pack, snapshot, and op-log entry points.
- **S4 (Path traversal and symlink escape — R5)**: **PASS (N/A)**. All pack, snapshot, and op-log operations in `aegis-model` operate purely on `std::io::Read` and `std::io::Write` streams without filesystem path resolution.
- **S5 (Customer data and PII in errors / logs — R2)**: **PASS**.
  - `ModelError::UnknownFeasibilityFactor(FeasibilityFactor)`, `ModelError::UnknownFactorLevel { factor, level }`, and `ModelError::AttackTreeHasFactors { tree, factors }` (`crates/aegis-model/src/error.rs:281-294`, `362-369`) carry only compact vocabulary indices (`u8`), `TreeId` (`UUIDv7`), and `usize` counts—never node labels, entity names, or actor text.
  - `MethodologyError` variants (`FeasibilityFactorCount`, `DuplicateFeasibilityFactor`, `FactorLevelCount`, `DuplicateFactorLevel`, `FactorPointsTooLarge`, `PotentialBandCount`, `PotentialBandRange`, `UnknownBandFeasibilityLevel` in `crates/aegis-model/src/error.rs:647-722`) carry only integer counts/bounds/indices and vocabulary keys either already validated by `check_key` (`<= 64` ASCII bytes) or truncated via `error_key_text` to at most `MAX_KEY_BYTES` (64 bytes, UTF-8 char boundary) and formatted with `{:?}` (debug-escaped).
  - `StoreError::LimitExceeded(Limit::FeasibilityFactors { max })` and `StoreError::UnknownVocabularyKey` (`crates/aegis-model/src/store/mod.rs:146-149`, `210-211`, `248-252`, `268-269`) report only the static limit or a length-checked (`<= 64` bytes) debug-escaped key.
  - `LintCode::AttackTreeStepIncompleteFactors` and `LintCode::RedundantStepFeasibility` (`crates/aegis-model/src/lint.rs:92-109`) carry only `TreeId` and `NodeId`.
- **S6 (Algorithmic complexity / DoS — P1, P2)**: **PASS**.
  - `MAX_FEASIBILITY_FACTORS = 16`, `MAX_LEVELS = 16`, `MAX_FACTOR_POINTS = 1_000`.
  - Score summation in `Methodology::resolve_attack_potential` (`crates/aegis-model/src/methodology/mod.rs:2519`) widens `u16` points (`<= 1_000`) to `u32` and sums at most 16 values (`<= 16_000`), which cannot overflow `u32`.
  - Band range contiguity check in `build_attack_potential` (`crates/aegis-model/src/methodology/mod.rs:2778-2816`) widens `u16` `min`/`max` to `u32` before computing `u32::from(m) + 1`, preventing `u16::MAX + 1` overflow.
  - Duplicate factor detection in `NodeFactors` (`crates/aegis-model/src/store/wire.rs:926-933`) uses a `u64` bitmask with `1u64.checked_shl(idx)`.
  - Per-step factor storage in `TreeEntry::step_factors` (`crates/aegis-model/src/project.rs:4131-4145`) allocates nothing (`Box::default()`) when unrated and at most `factor_count` (`<= 16`) `Option<FactorLevel>` bytes when rated, collapsing back to `Box::default()` when the last factor rating is cleared.
- **S7 (Deterministic iteration order — P4, D7)**: **PASS**.
  - `AttackPotential` uses `BTreeMap<Arc<str>, FeasibilityFactor>` and `BTreeMap<Arc<str>, FactorLevel>` for key lookups (`crates/aegis-model/src/methodology/mod.rs:1076-1093`).
  - Snapshot serialization in `StepFactorsByNode` and `StepFactorsForNode` (`crates/aegis-model/src/store/snapshot.rs:463-522`) iterates tree nodes in deterministic arena order and factors in canonical `KeyOrder` (`order.feasibility_factors`, sorted by factor key).
  - Neither `HashMap` nor `HashSet` was introduced in any modified module.
- **S8 (Provenance integrity — R7)**: **PASS**.
  - `Op::SetStepFactor` carries a mandatory `Provenance` field (`crates/aegis-model/src/op.rs:297-309`), validated via `check_provenance` in `Project::validate` (`crates/aegis-model/src/project.rs:2885`).
  - `commit_set_step_factor` (`crates/aegis-model/src/project.rs:4146-4153`) updates `entry.provenance` and returns the previous provenance and previous `Option<FactorLevel>` in the inverse `Op::SetStepFactor`, preserving exact undo/redo provenance symmetry.
  - Op-log wire serialization and deserialization (`crates/aegis-model/src/store/wire.rs:417-423`, `crates/aegis-model/src/store/op_log.rs:437-449`, `587-598`) preserve `SetStepFactor` provenance end-to-end.
- **S9 (Subprocess / command injection)**: **PASS (N/A)**. No subprocess execution (`std::process::Command`) is used.
- **S10 (IPC boundary validation)**: **PASS (N/A)**. Pure Rust domain model crate; no Tauri IPC commands are defined here.
- **S11 (Dependency policy — D5, Q7)**: **PASS**. `crates/aegis-model/Cargo.toml` and `deny.toml` are unchanged; no new dependencies were added.

## Findings

None.
