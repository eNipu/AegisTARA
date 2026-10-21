## Round 1
Commits: 1b04165 Add methodology-neutral TARA core with typed, invertible ops
Files:
- crates/aegis-model/src/id.rs (shared `check_v7`/`parse_v7`, `entity_ids!` macro for AssetId/AssetIdx, DamageId/DamageIdx, ThreatId/ThreatIdx; NodeId/NodeIdx behave as before)
- crates/aegis-model/src/methodology/mod.rs (MethodologyDef and parts, Methodology, vocabulary indices, lookups, limits)
- crates/aegis-model/src/methodology/catalog.rs (CatalogSyntax, SegmentDef, CatalogRef, parse/accept/display)
- crates/aegis-model/src/methodology/iso21434.rs (`Methodology::iso21434_r155()`, the only non-test file with 21434/R155 vocabulary)
- crates/aegis-model/src/asset.rs (PropertySet, Asset)
- crates/aegis-model/src/damage.rs (DamageScenario)
- crates/aegis-model/src/threat.rs (ThreatScenario, MAX_CATALOG_REFS)
- crates/aegis-model/src/op.rs (Op, 16 variants, #[non_exhaustive])
- crates/aegis-model/src/project.rs (Project, arenas with tombstones, BTreeMap ID maps, ref-count cache, validate/apply/commit, PartialEq, check_invariants, lib proptests)
- crates/aegis-model/src/error.rs (26 new ModelError variants, new MethodologyError; existing variants and texts unchanged)
- crates/aegis-model/src/lib.rs (modules, re-exports, crate-doc "Methodologies" section with example)
- crates/aegis-model/tests/common/mod.rs (shared test kit: built-in, second, and "wide" methodologies; deterministic UUIDv7s; random op generator; `replay` harness)
- crates/aegis-model/tests/project.rs, tests/second_methodology.rs, tests/ops_properties.rs
- docs/adr/0008-methodology-neutral-domain-model.md

Acceptance criteria:
- [x] AC1 — `cargo test --lib ids_` → 5 passed (v7-only, Display/FromStr round trip for all three new IDs plus NodeId). `cargo test --doc` → 181 passed, incl. `id::AssetId - compile fail` (passing `&AssetId` to `threat_scenario_idx`); verified separately that the snippet fails with E0308 "expected `&ThreatId`, found `&AssetId`".
- [x] AC2 — `cargo test --lib methodology_rejects_` → 23 passed; every listed case has its own MethodologyError variant (per-vocabulary duplicate variants, DuplicateLevel, InvalidKey, BlankLabel, LabelTooLong, InvalidVersion, LevelCount, No*/TooMany* counts, Empty/Unknown/RepeatedViolates, CatalogSegmentCount/Required/SegmentRange/SegmentDigits/Prefix).
- [x] AC3 — `cargo test --lib iso21434_vocabulary` → 5 passed (keys, order, 4 levels lowest first, STRIDE single-property mapping, scheme; `Methodology::new(definition()) == iso21434_r155()`).
- [x] AC4 — `cargo test --lib catalog_syntax` → 8 passed (R155 accept/reject lists and `4 < 4.1 < 4.2 < 5` in iso21434.rs; CAPEC- and ATT&CK-style schemes in catalog.rs; display round trips for every accepted ID).
- [x] AC5 — `cargo test --lib value_sizes` → 1 passed, `cargo test --lib property_set` → 5 passed. Measured: PropertySet 8, CatalogRef 10, each index 1 byte.
- [x] AC6 — `cargo test --test project worked_example` → passed (2 assets, 2 damage scenarios one partly unrated, 2 threats with R155 refs; sorted damages/refs regardless of add order; unrated is None).
- [x] AC7 — `cargo test --test second_methodology` → 3 passed (vocabulary shape 3/5/2 levels, C/I/A, two-property `ransomware`, CAPEC; every Op variant applied; built-in indices property 5, level 3 in the 3-level scale, category 3, threat category 5, and a 2-segment R155 ref rejected).
- [x] AC8 — `cargo test --test project rejects_` → 4 passed; `rejects_every_new_variant` triggers all 26 new variants (asserted against an explicit list) and checks `validate == apply` and `project == clone_before`.
- [x] AC9 — `cargo test --test project referential_integrity` → 2 passed.
- [x] AC10 — `cargo test --test project inverse_round_trip` → passed; all 16 Op variants (asserted), apply+inverse == before, inverse of inverse == op, redo == after.
- [x] AC11 — `cargo test --test ops_properties` → 3 passed (256 cases per methodology; plus a deterministic coverage test: ~36% / ~33% of generated ops succeed, all 16 op kinds succeed, 25 of 26 error variants occur; the capacity error is covered in tests/project.rs).
- [x] AC12 — `cargo test --lib project_invariants` → 2 passed (256 cases per methodology, `check_invariants` after every op, every inverse check, and every undo).
- [x] AC13 — `cargo test --test project deterministic_order` → passed (two orders incl. a tombstone; `==`; iterators yield the same ascending IDs; stored lists equal by public ID).
- [x] AC14 — `cargo test --test project errors_have_no_labels` → passed (all 26 variants with every entity named `SECRET-LABEL-9f3`; Display and Debug never contain it).
- [ ] AC15 — the grep prints exactly one line: `src/methodology/mod.rs:12:mod iso21434;`. See Notes; everything else (including `stride` identifiers, which matched "stride", and doc examples passed as string literals) was fixed.
- [x] AC16 — `cargo test --doc` → 181 passed; `cargo clippy --all-targets -- -D warnings` → clean. Every new public item has a doc example; project- and methodology-sized functions state their cost.
- [x] AC17 — `grep -rnE '\.unwrap\(|\.expect\(|panic!' src/methodology src/asset.rs src/damage.rs src/threat.rs src/op.rs src/project.rs` → hits only in `#[cfg(test)] mod tests` blocks (catalog.rs:376+, mod.rs:1401+, iso21434.rs:163+, project.rs:1542+). Arena, count, and vocabulary access goes through `get`/`get_mut`.
- [x] AC18 — `cargo test` → 364 passed, 0 failed (existing test files untouched); `cargo test --benches` → exit 0.
- [x] AC19 — `test -f docs/adr/0008-... && grep -qi 'second methodology' ...` → exit 0. Covers principle, enums vs data and its costs, TVRA-style sketch plus what still needs code, Open questions 1-3, grammar vs regex on R155/CAPEC/ATT&CK, limits, inverse ops vs snapshots, tombstones vs swap_remove, BTreeMap maps, ref counts vs scanning, measured sizes.
- [x] AC20 — `scripts/ci.sh crates/aegis-model` → all steps passed; `scripts/deny.sh licenses bans sources` → ok; `git diff --exit-code master -- crates/aegis-model/Cargo.toml crates/aegis-model/Cargo.lock` → exit 0 (no dependency change).

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (364 tests + 181 doctests)
- `cargo test --benches` → pass
- `scripts/ci.sh crates/aegis-model` → pass
- `scripts/deny.sh licenses bans sources` → pass
- neutrality grep → fail: prints one line, `crates/aegis-model/src/methodology/mod.rs:12:mod iso21434;` (see Notes)
- `test -f docs/adr/0008-methodology-neutral-domain-model.md` → pass

Notes:
- **AC15 false positive.** The spec puts the built-in methodology in `src/methodology/iso21434.rs`, so some non-comment line elsewhere in `src` has to declare that module, and the grep reports that line. Every way of declaring it (`mod iso21434;`, `#[path = "iso21434.rs"]`, `include!`) contains "21434". I could hide it (an escaped `#[path]` string, or a declaration in a non-`.rs` file), but that only gets around the check, so I didn't. Two options: amend the check with `| grep -v ':mod iso21434;$'`, or rename the file to a neutral name and change the check's exclusion to match. I'd take the first. The ADR's Consequences section records this too.
- **Second methodology location (AC7).** `MethodologyDef` for the medical-device-style methodology is `second_def()` in `tests/common/mod.rs`, not inline in `tests/second_methodology.rs`. The AC11 and AC12 proptests replay ops over the same definition, so it needs one shared home. `second_methodology.rs` exercises it and says where it is defined. No library file references it.
- **Shared test kit in the lib tests.** `src/lib.rs` has `#[cfg(test)] extern crate self as aegis_model;` and `#[cfg(test)] #[path = "../tests/common/mod.rs"] mod testkit;`. This lets the AC12 lib proptest reuse the same generator as the AC11 integration proptest. Since only the kit calls `iso21434_r155()`, `src/project.rs` stays neutral. The kit uses only the public API and has `#![allow(clippy::pedantic)]` (test code).
- **Added API beyond the spec:** `Project::with_slot_limit(methodology, limit)` (default `u32::MAX`). Integration tests had no other way to reach `ProjectCapacityExceeded`, and it bounds projects built from untrusted input (R6). Equality ignores the limit. Also added: `Methodology::{impact_category_count, level_count, property_count, threat_category_count, catalog_scheme_count, all_properties, catalog_syntax}`, `index()` on every vocabulary index, `CatalogRef::{scheme, segments}`, and the public consts `MAX_NAME_CHARS` and `MAX_CATALOG_REFS`.
- **Third reference count.** Besides the asset x property counts (flat, row length = property count) and the per-damage counts, there is a per-asset threat count. It gives `AssetInUse { threats }` without a scan.
- **Check order** in `validate`: ID, then name, then vocabulary ranges, then entity rules, then referential rules, then capacity. For `AddThreatScenario`, `TooManyCatalogRefs` is checked before each ref's syntax. Invalid vocabulary in remove/unlink-style ops (for example a foreign `CatalogRef` in `RemoveCatalogRef`) is reported as a vocabulary error before "not found".
- **Inverse of inverse (AC10)** equals the original op only for ops in canonical form: `AddThreatScenario` with sorted refs and damages, and `AddDamageScenario` impacts in category order. The inverse of a remove uses stored order, as the spec says. The AC11 proptests check state restoration only, not op equality.
- `MethodologyError::InvalidKey` carries the bad key as given. That is vocabulary, not project data.
- Timing benchmarks for the new ops were out of scope. The ADR records measured type sizes and generator coverage numbers.

## Round 2
Commits: 69014b3 Address round 1 review: lazy catalog errors, scoped constructors, no lint allows
Files: crates/aegis-model/src/methodology/catalog.rs, crates/aegis-model/src/methodology/mod.rs, crates/aegis-model/src/project.rs
Review items (round-1/review-code.md and rework.md `## Rework 1`):
- [x] AC15 blocker: the check was corrected in rework.md. The corrected command prints nothing. No code change.
- [x] catalog.rs `CatalogSyntax::parse`: the error is now built lazily, through `let err = || ModelError::InvalidCatalogId { scheme }` and `ok_or_else(err)`. Nothing is cloned per segment.
- [x] mod.rs `#[allow(dead_code)]` on the macro constructor: removed. The `vocab_index!` macro no longer defines `new`. Each type now has only the constructor it needs:
  - `ImpactCategory::new` and `Property::new` are `pub(crate)`, used by `DamageScenario::impacts` and `PropertySet::iter`.
  - `Level::new` and `CatalogScheme::new` are `#[cfg(test)]`.
  - `ThreatCategory` has no raw constructor, since nothing needs one.
- [x] Four `clippy::type_complexity` allows: removed. A private `struct Vocab<T> { items, by_key }` is now the return type of the four `build_*` helpers.
- [x] `clippy::too_many_arguments` allow on `check_add_threat` (review finding 5): removed. The method has 7 parameters including `self`, which is within clippy's limit, so the allow was never needed. No `#[allow(...)]` remains in `src/methodology` or `src/project.rs`.
- Findings 4, 6 and 7 were accepted by the reviewer and left as they were.
- Optional UX items (review-ux.md) were not done. Each one would add a field to a `ModelError` variant whose shape the spec fixed (`EmptyName`, `ProjectCapacityExceeded`, `CatalogRefNotFound { threat }`, `DuplicateCatalogRef { threat }`). Showing the expected syntax or the ref text also needs the methodology, which the error does not hold. The UI can add this context when it maps errors, using `Methodology::catalog_scheme_key`, `catalog_syntax` and `display_catalog_ref`. I'd rather change the variant shapes in a separate, explicit job.
Acceptance criteria: unchanged from Round 1. AC15 now passes:
- [x] AC15 — the corrected grep from rework.md (`... | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'`) prints nothing.
- AC17 re-checked: every `unwrap`, `expect` or `panic!` hit sits after the last `#[cfg(test)]` line in each file (catalog.rs 376 > 316, iso21434.rs 163 > 157, mod.rs 1423 > 1349, project.rs 1544 > 1517).
Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (364 passed, 0 failed, including doctests)
- `cargo test --benches` → pass
- `scripts/ci.sh crates/aegis-model` → pass
- `scripts/deny.sh licenses bans sources` → pass
- corrected neutrality grep (rework.md) → pass (prints nothing)
- `test -f docs/adr/0008-methodology-neutral-domain-model.md` → pass
- `git diff --exit-code master -- crates/aegis-model/Cargo.toml crates/aegis-model/Cargo.lock` → pass
Notes: these are internal refactors only. The public API and behaviour are unchanged, and every round-1 test passes as before.
