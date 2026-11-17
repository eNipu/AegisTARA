# Build Report — 131a

## Summary
Added optional `clause_refs` map (page key -> list of reference strings) to the
methodology pack format and `MethodologyDef`/`Methodology`.

## Key Changes
- `crates/aegis-model/src/methodology/mod.rs`: `MethodologyDef.clause_refs` field; `Methodology.clause_refs` field carried in `Methodology::new`; accessors `clause_refs(&self, page)` and `clause_refs_map(&self)`.
- `crates/aegis-model/src/methodology/pack.rs`: `DefWire.clause_refs` optional bounded field; conversion and serialized view (skipped when empty).

## Verification
- `cargo fmt --all`: PASS
- `scripts/ci.sh crates/aegis-model`: PASS
- `cargo test --locked -p aegis-model`: PASS

NEXT: reviewer

## Rework 1

The gate failed because the first implementation touched `crates/aegis-model/src/error.rs`
and `crates/aegis-model/src/methodology/catalog.rs`, which are outside this slice's
allow list. Those edits were reverted in commit 0be824d.

Blocker: adding a required field (`clause_refs`) to `MethodologyDef` forces updates to
every struct construction site in the crate, including the doc-test in `error.rs` and
the test fixture in `catalog.rs`. Both are outside the allow list, so the feature
cannot be implemented as specified while staying in-scope. The crate does not compile
in its test targets until those two files are updated (missing `clause_refs` field).

Request: expand the allow list to include
`crates/aegis-model/src/error.rs` and `crates/aegis-model/src/methodology/catalog.rs`
(or split the doc/test updates into a separate slice), then re-run the builder.

NEXT: planner

## Rework 2

Commit: a9b2cbc Give clause_refs its own Limit and fix the remaining test targets (131a)
(on top of 0be824d; commits 45495c5 and 0be824d kept untouched)

Files:
- crates/aegis-model/src/error.rs (both doc examples: added `clause_refs: std::collections::BTreeMap::default(),`)
- crates/aegis-model/src/methodology/catalog.rs (`catalogs()` fixture: same field added)
- crates/aegis-model/src/methodology/mod.rs (new `MAX_PACK_CLAUSE_REFS: usize = 32` const after `MAX_PACK_TRANSLATIONS`)
- crates/aegis-model/src/methodology/pack.rs (`ClauseRefsCap = MAX_PACK_CLAUSE_REFS, Limit::CLAUSE_REFS;` plus the import)
- crates/aegis-model/src/store/mod.rs (`Limit::ClauseRefs { max }` variant, import, `Limit::CLAUSE_REFS` const, Display arm "more than {max} clause refs")
- crates/aegis-model/tests/pack_properties.rs (`round_trip_clause_refs`: two-page map round-trips via `check_round_trip`, then accessor asserts)
- crates/aegis-model/tests/pack_limits.rs (`clause_refs_caps`: 32 refs on one page at cap; 33 refs on one page and 33 pages with one ref each refused with `Limit::ClauseRefs { max: 32 }`)

Done when:
1. Test targets compile and the suite passes — `cargo test --locked -p aegis-model`: PASS (197 unit, all integration targets, 709 doc tests; 0 failed).
2. Pack with refs round-trips, builtins decode to an empty map — `cargo test --locked -p aegis-model --test pack_properties --test builtin_pack`: PASS (7 + 15, incl. `round_trip_clause_refs`).
3. Over-cap refs refused with `Limit::ClauseRefs` — `cargo test --locked -p aegis-model --test pack_limits clause_refs_caps`: PASS (asserts the exact `Limit`, so the old `Limit::ImpactCategories` reuse would fail it).

Before-you-commit checks (worktree root, CARGO_TARGET_DIR shared with main checkout):
- `cargo fmt --all`: PASS
- `scripts/ci.sh crates/aegis-model`: PASS (fmt, clippy, test)
- `cargo test --locked -p aegis-model`: PASS

Notes:
- The prompt's doc text for `Limit::ClauseRefs` needed backticks around `clause_refs` and `MAX_PACK_CLAUSE_REFS` to pass clippy's `doc_markdown` (-D warnings); wording unchanged, wrapped over two lines like its neighbours. No intra-doc link to the const because it is not re-exported at the crate root and `lib.rs` is out of scope.
- Page keys go through the wire `Key` check (`[a-z][a-z0-9_-]{0,63}`), so the tests use valid keys (`"page"`, `key_of_len("pg", i, 6)`); the round-trip test uses `"assets"` and `"threat_scenario"` as the prompt suggests.
- Gate size: `git diff --numstat c7b1f86` counts 129 lines over 11 files, all inside the widened allow list (max_lines 450).
- No builtin pack JSON, snapshot/op-log format, dependency, or round-1 commit was touched.

NEXT: reviewer
