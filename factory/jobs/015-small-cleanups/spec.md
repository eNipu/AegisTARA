# Job 015: Small cleanups (proptest failure persistence & specific `ModelError` messages)

**Goal:** Complete the two small cleanups in `factory/backlog.md`:
1. Point proptest failure persistence in `crates/aegis-model/tests/properties.rs` at a fixed file (`proptest-regressions/properties.txt`), matching `crates/aegis-model/tests/eval_properties.rs`.
2. Make the four `ModelError` messages noted in the job 009 UX review (`factory/jobs/009-domain-model-v0-2/round-1/review-ux.md`) more specific—without ever including entity names:
   - Which entity's name was empty or too long (`ModelError::EmptyName { kind }`, `ModelError::NameTooLong { kind, max }`).
   - What catalog syntax was expected (`ModelError::InvalidCatalogId { scheme, syntax }`, with `Display` on `CatalogSyntax` and `SegmentDef`).
   - Which project capacity limit was hit (`ModelError::ProjectCapacityExceeded { kind }`).
   - Which catalog ref was missing or duplicated (`ModelError::CatalogRefNotFound { threat, catalog_ref }`, `ModelError::DuplicateCatalogRef { threat, catalog_ref }`, with `Display` on `CatalogRef`).

## Scope (`crates/aegis-model`)

### 1. Proptest failure persistence (`tests/properties.rs`)
- Replace `#![proptest_config(ProptestConfig::with_cases(32))]` with `#![proptest_config(config(32))]` using `FileFailurePersistence::Direct("proptest-regressions/properties.txt")` and honoring `PROPTEST_CASES`, matching `tests/eval_properties.rs`.

### 2. `EntityKind` for name and capacity errors
- Add `EntityKind` (`#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)] #[non_exhaustive]`):
  - `Asset` (`"asset"`)
  - `DamageScenario` (`"damage scenario"`)
  - `ThreatScenario` (`"threat scenario"`)
  - `AttackTree` (`"attack tree"`)
  - `TreeNode` (`"tree node"`)
- Re-export `EntityKind` at the crate root.
- Update `ModelError::EmptyName { kind: EntityKind }` (`"{kind} name is empty"`), `ModelError::NameTooLong { kind: EntityKind, max: usize }` (`"{kind} name is longer than {max} chars"`), and `ModelError::ProjectCapacityExceeded { kind: EntityKind }` (`"project {kind} capacity exceeded"`).

### 3. Expected catalog syntax in `ModelError::InvalidCatalogId`
- Implement `fmt::Display` for `SegmentDef` (`<min..=max>`, zero-padded when `digits` is `Some(w)`) and `CatalogSyntax` (`prefix` followed by required segments separated by `.` and optional segments in `[.seg]`).
- Add `syntax: CatalogSyntax` to `ModelError::InvalidCatalogId { scheme, syntax }` and format it in the error message (`"catalog ID does not match syntax {syntax} of scheme index {}"`).

### 4. Specific catalog ref in `CatalogRefNotFound` and `DuplicateCatalogRef`
- Implement `fmt::Display` for `CatalogRef` (its segments joined by `.`).
- Add `catalog_ref: CatalogRef` to `ModelError::CatalogRefNotFound { threat, catalog_ref }` (`"threat scenario {threat} does not carry catalog ref {catalog_ref}"`) and `ModelError::DuplicateCatalogRef { threat, catalog_ref }` (`"threat scenario {threat} has duplicate catalog ref {catalog_ref}"`).

## Acceptance Criteria

- **AC1:** `crates/aegis-model/tests/properties.rs` configures `FileFailurePersistence::Direct("proptest-regressions/properties.txt")` and respects `PROPTEST_CASES`.
- **AC2:** `EmptyName`, `NameTooLong`, and `ProjectCapacityExceeded` carry `kind: EntityKind` and include it in their `Display` output for assets, damage scenarios, threat scenarios, attack trees, and tree nodes.
- **AC3:** `InvalidCatalogId` carries `syntax: CatalogSyntax` and includes the formatted syntax in its `Display` output.
- **AC4:** `CatalogRefNotFound` and `DuplicateCatalogRef` carry `catalog_ref: CatalogRef` and include it in their `Display` output.
- **AC5:** No `ModelError` message or `Debug` output ever contains an entity name (`Asset::name`, `DamageScenario::name`, `ThreatScenario::name`, `Node::label`) or actor name (`Provenance::actor`).
- **AC6:** `./scripts/ci.sh`, `cargo test --benches`, and `./scripts/deny.sh` pass with zero warnings.
