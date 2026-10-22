# Build: `015-small-cleanups`

## Branch and commit
- Branch: `factory/015-small-cleanups`
- Commit: `9d6163e` (`Make ModelError messages specific and persist proptest regressions`)

## Changes
- `crates/aegis-model/tests/properties.rs`:
  - Added `config(cases: u32) -> ProptestConfig` using `FileFailurePersistence::Direct("proptest-regressions/properties.txt")` and honoring `PROPTEST_CASES`, matching `tests/eval_properties.rs`.
- `crates/aegis-model/src/error.rs` & `crates/aegis-model/src/lib.rs`:
  - Added `EntityKind` (`Asset`, `DamageScenario`, `ThreatScenario`, `AttackTree`, `TreeNode`, `#[non_exhaustive]`) with `as_str()` and `fmt::Display`.
  - Updated `ModelError`:
    - `EmptyName { kind: EntityKind }` -> `"{kind} name is empty"`
    - `NameTooLong { kind: EntityKind, max: usize }` -> `"{kind} name is longer than {max} chars"`
    - `ProjectCapacityExceeded { kind: EntityKind }` -> `"project {kind} capacity exceeded"`
    - `InvalidCatalogId { scheme: CatalogScheme, syntax: CatalogSyntax }` -> `"catalog ID does not match syntax {syntax} of scheme index {}"`
    - `CatalogRefNotFound { threat: ThreatId, catalog_ref: CatalogRef }` -> `"threat scenario {threat} does not have catalog ref {catalog_ref}"`
    - `DuplicateCatalogRef { threat: ThreatId, catalog_ref: CatalogRef }` -> `"threat scenario {threat} already has catalog ref {catalog_ref}"`
- `crates/aegis-model/src/methodology/catalog.rs` & `crates/aegis-model/src/methodology/mod.rs`:
  - Implemented `fmt::Display` for `SegmentDef`, `CatalogSyntax`, and `CatalogRef`.
  - Updated `CatalogSyntax::parse` and `Methodology::check_catalog_ref` to include `syntax: self.clone()` on `ModelError::InvalidCatalogId`.
- `crates/aegis-model/src/project.rs` & `crates/aegis-model/tests/*.rs`:
  - Updated `check_name`, capacity checks, catalog ref checks, and test suites (`tests/project.rs`, `tests/builtin_pack.rs`, `tests/second_methodology.rs`), including `specific_model_error_messages`.

## Verification
- `./scripts/ci.sh` -> all steps passed (`aegis-model fmt`, `aegis-model clippy`, `aegis-model test`: 159 unit tests, 77 integration tests, 8 property tests, 89 doctests).
- `cargo test --manifest-path crates/aegis-model/Cargo.toml --benches` -> passed.
- `./scripts/deny.sh` -> passed (`aegis-model`, `aegis-model/fuzz`).
