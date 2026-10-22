VERDICT: PASS

## UX & domain review: `015-small-cleanups`

- Resolves all four UX findings from job `010-project-model` (`factory/jobs/010-project-model/reviews/ux.md`):
  1. `EmptyName` and `NameTooLong` now state which entity kind had the invalid name (`asset`, `damage scenario`, `threat scenario`, or `tree node`).
  2. `InvalidCatalogId` now prints the expected syntax pattern (`<1..=32>[.<1..=99>]`, `CAPEC-<1..=9999>`, `T<0001..=9999>[.<001..=999>]`).
  3. `ProjectCapacityExceeded` now names which arena hit the slot limit (`asset`, `damage scenario`, `threat scenario`, or `attack tree`).
  4. `CatalogRefNotFound` and `DuplicateCatalogRef` now identify which catalog reference (`4.1`, `5`, etc.) was missing or duplicated.
