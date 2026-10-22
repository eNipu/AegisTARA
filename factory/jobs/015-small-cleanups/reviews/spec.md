VERDICT: PASS

## Spec compliance: `015-small-cleanups`

- **AC1 (proptest failure persistence)**: `crates/aegis-model/tests/properties.rs` defines `config(cases: u32) -> ProptestConfig` pointing at `"proptest-regressions/properties.txt"` via `FileFailurePersistence::Direct` and honoring `PROPTEST_CASES`, matching `tests/eval_properties.rs`.
- **AC2 (`EmptyName` & `NameTooLong` carry `EntityKind`)**: `EntityKind` (`Asset`, `DamageScenario`, `ThreatScenario`, `AttackTree`, `TreeNode`, `#[non_exhaustive]`) is carried by `EmptyName { kind }` and `NameTooLong { kind, max }`, rendering `"{kind} name is empty"` and `"{kind} name is longer than {max} chars"`.
- **AC3 (`InvalidCatalogId` carries `CatalogSyntax`)**: `InvalidCatalogId { scheme, syntax }` renders `"catalog ID does not match syntax {syntax} of scheme index {}"` using `CatalogSyntax`'s `Display` implementation (e.g., `<1..=32>[.<1..=99>]`).
- **AC4 (`ProjectCapacityExceeded` carries `EntityKind`)**: `ProjectCapacityExceeded { kind }` renders `"project {kind} capacity exceeded"` for each arena (`Asset`, `DamageScenario`, `ThreatScenario`, `AttackTree`).
- **AC5 (`CatalogRefNotFound` & `DuplicateCatalogRef` carry `CatalogRef`)**: Both carry `catalog_ref: CatalogRef` and render its segments via `CatalogRef`'s `Display` implementation.
- **AC6 (No entity names in errors)**: `errors_never_contain_names` and `specific_model_error_messages` verify that `SENTINEL` never appears in `Display` or `Debug` output of any `ModelError`.
- **AC7 (CI & deny)**: `./scripts/ci.sh`, `cargo test --benches`, and `./scripts/deny.sh` pass cleanly.
