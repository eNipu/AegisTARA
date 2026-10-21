VERDICT: CHANGES

Reason: one spec check fails (the AC15 neutrality grep). It cannot be made to pass by builder code changes (see finding 1), so it needs a spec/check amendment from the caller. The code itself is sound; the other findings are minor.

## Check results (run from factory/worktrees/009-domain-model-v0-2)

- `cargo fmt --check` -> pass
- `cargo clippy --all-targets -- -D warnings` -> pass
- `cargo test` -> pass (119 lib, 3+10+8+10+3+11+12(project)+2+3 integration/other, 181 doctests, 0 failed)
- `cargo test --benches` -> pass
- `scripts/ci.sh crates/aegis-model` -> pass (fmt, clippy, test)
- `scripts/deny.sh licenses bans sources` -> pass
- Neutrality grep (AC15) -> FAIL: prints `crates/aegis-model/src/methodology/mod.rs:12:mod iso21434;`
- `test -f docs/adr/0008-methodology-neutral-domain-model.md` -> pass; contains "second methodology"
- `git diff master -- crates/aegis-model/Cargo.toml Cargo.lock` -> empty (no dependency change)
- AC17 grep for unwrap/expect/panic -> all hits are inside `#[cfg(test)] mod tests` (mod.rs:1401+, project.rs:1545+, iso21434.rs:163+, catalog.rs:376+). Panicking indexing (`w[0]`) appears only in the cfg(test) `check_invariants`.
- Per-AC named tests (each re-run): ids_ 5, methodology_rejects_ 23, iso21434_vocabulary 5, catalog_syntax 8, value_sizes 1, property_set 5, worked_example 1, second_methodology 3, rejects_ 4, referential_integrity 2, inverse_round_trip 1, ops_properties 3 (256 cases per methodology), project_invariants 2 (256 cases), deterministic_order 1, errors_have_no_labels 1 -> all pass.

## Acceptance criteria

All of AC1-AC14, AC16-AC20 are met and tested. AC15 fails as a literal check (finding 1); the substance (no automotive vocabulary in core code) is met.

## Findings

1. `crates/aegis-model/src/methodology/mod.rs:12` — AC15 check prints `mod iso21434;` — This is a defect in the spec's check, not in the code: any declaration of a module file named `iso21434.rs` contains "21434" on a non-comment line outside that file (`#[path]` and `include!` also contain it). I verified there is no other hit, and the builder correctly did not hide it. Fix (caller, not builder): amend the check to `| grep -v ':mod iso21434;$'` (builder's suggestion) and re-run. No code change needed. Counted as CHANGES only because a listed check fails.

2. `crates/aegis-model/src/methodology/mod.rs:223` — `#[allow(dead_code)]` on `vocab_index!::new` — Not every index type uses `new` outside tests (`Level`, `ThreatCategory`, `CatalogScheme` appear to be built through the tuple constructor in this module and `new` is only reached from tests), so the allow hides code that is test-only. Minor. Fix: drop `new` for the unused types or put it behind `#[cfg(test)]`, and remove the allow.

3. `crates/aegis-model/src/methodology/catalog.rs:167,171,172,173` — `ok_or(err.clone())` evaluates eagerly: `err` is cloned on every segment iteration even on success (R1). Fix: build the error lazily (`ok_or_else(|| ModelError::InvalidCatalogId { scheme })`), or use a small closure; the error is just `scheme` (Copy), so no clone is needed at all.

4. `crates/aegis-model/src/project.rs:1083,1091` — `validate` copies the ref and damage slices (`to_vec`) to sort them for the duplicate check. This is within the documented O(k log k) cost and `&Op` forces it, so it is acceptable; noting it only as a possible R1 point. No fix required.

5. `crates/aegis-model/src/project.rs:1058`, `methodology/mod.rs:526,571,594,640` — four `#[allow(clippy::type_complexity)]` and one `#[allow(clippy::too_many_arguments)]`. Minor. Fix: a small `struct Vocab<T> { items: Vec<T>, by_key: BTreeMap<Arc<str>, u8> }` removes the four type_complexity allows and the repeated return tuple.

6. `crates/aegis-model/src/project.rs` (`Project::with_slot_limit`) and extra `Methodology` getters (`impact_category_count`, `level_count`, `all_properties`, `catalog_syntax`, ...) — public API the spec did not list. Justified in build.md (capacity test needs it; R6) and documented with examples, so I accept it. If the caller wants strict scope, `with_slot_limit` could be `#[doc(hidden)]` or `pub(crate)` plus a lib test.

7. `crates/aegis-model/tests/common/mod.rs` — the second methodology's `MethodologyDef` lives in the shared test kit, not in `tests/second_methodology.rs` as AC7 words it. Deviation is reasonable (AC11/AC12 reuse it) and it is test-only, no library file refers to it. Accepted.

## Notes (no action)

- Logic review of `validate`/`apply`/`commit`: commits cannot fail after validation; reference counts (per asset x property, per asset, per damage) are updated in every commit path that changes links, category, or entity lifetime; sorted `damages` by `DamageId` and sorted unique `catalog_refs` are kept by binary-search insert/remove; inverses carry the old values. Equality is logical (IDs and content, tombstones ignored). No O(n) work per op (A9); big-O is documented on the public items.
- Catalog grammar: leading-zero, whitespace, sign, empty-part, extra-segment and out-of-range cases are all rejected by `SegmentDef::parse`; `accepts` re-checks foreign refs including zeroed unused segments.
- Standards: arenas and typed IDs (D1, D2), `BTreeMap` for output order (D7, P4), Arc<str> (D5), and ref counts as a documented cache (D10) are followed. ADR 0008 covers all items the spec lists.
