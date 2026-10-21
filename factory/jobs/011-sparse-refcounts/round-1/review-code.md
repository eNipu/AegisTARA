VERDICT: PASS

Reviewed branch `factory/011-sparse-refcounts` (ce093fa) from `factory/worktrees/011-sparse-refcounts/`.

## Check results

- `cargo fmt --check` -> pass
- `cargo clippy --all-targets -- -D warnings` -> pass
- `cargo test` -> pass (all result lines ok)
- `cargo test --locked` -> pass
- `cargo test --benches` -> pass
- `cargo bench --bench store_memory` -> pass. All 18 cases are within their gates. The numbers match build.md and ADR 0010 (`assets_only_max` 2.41 peak / 1.78 kept, `damages_only_max` 3.53 / 2.65, `targeted_narrow_max` 2.57 / 1.76, `targeted_wide_max` 1.57 / 1.36).
- `cargo bench --bench store` -> not re-run. The AC5 timings are taken from build.md and ADR 0010 (three master and three branch runs, within +10%). The `RefCounts` change is O(r + p) with r, p <= 64, so they are plausible.
- `scripts/ci.sh crates/aegis-model` -> pass ("all steps passed")
- `scripts/deny.sh licenses bans sources` -> pass (aegis-model and fuzz)
- `rustup run 1.85 cargo check --locked --all-targets` -> pass
- `git diff --name-only master...HEAD -- crates/aegis-model` -> only `benches/store_memory.rs`, `src/project.rs`, `src/store/snapshot.rs`. Nothing under `tests/` or `fuzz/`, and no `Cargo.toml` or `Cargo.lock`.
- AC1 `pub` diff grep -> empty
- AC7 awk/grep for panics in non-test `project.rs` -> empty; `forbid(unsafe_code)` present
- AC8 neutrality grep -> empty
- AC4 grep for the old `8.3` gate -> empty
- AC6 doc greps -> all pass (ADR 0010 exists and mentions `assets_only_max`; ADR 0009 and 0008 contain `0010`; no `does **not** hold`; no `6.16x`, `394 MiB` or `reference-count row` in `snapshot.rs`)
- Targeted tests: `refcounts_are_sparse`, `check_invariants_detects_bad_property_counts`, `check_invariants_detects_a_stale_count`, `project_invariants_first_methodology` and `project_invariants_second_methodology` (256 cases each), `count_row_keeps_canonical_form`, `row_count_looks_up_by_property`, `refcounts_ignore_missing_asset_slots`, `refcounts_category_pair_shares_a_property_in_second` -> all pass.

## Acceptance criteria

- AC1 met. The API, errors and formats are unchanged, and the existing tests pass unchanged.
- AC2 met. `assert_sparse` covers (a) to (f) over `builtin()` and `second()`. (d) picks the category pair with the most shared properties and checks the undo op.
- AC3 met. `check_invariants` recounts through an independent `BTreeMap<(slot, property), count>`, not `count_row`. It checks canonical form per row, then compares exactly. The corruption test covers a wrong count, a zero entry and an out-of-order row.
- AC4 met.
  - `SERVER_BUDGET = 3.0` is defined, and every case has a `max_kept`.
  - The configuration check on loading cases is present.
  - `assets_only_max` has gates 3.3 (peak) / 2.2 (kept).
  - No gate is above its job 010 value.
  - The three new cases exist.
  - The bite tests are recorded in build.md. I read the gate code and it is correct, but did not repeat the bite tests because I may not edit code.
- AC5 not independently re-run; the recorded numbers pass the +10% limit.
- AC6 met. The ADR has context, options with bytes, the decision, a before/after table for all 18 cases, the timings and the complexity statement. The `RefCounts` doc comment states the space and time costs (A8).
- AC7 to AC9 met.

## Code review

- Logic of `count_row`, `new_row_len`, `row_count` and `RefCounts::property`:
  - The in-place path is correct. Counting in a subset only touches existing entries. Counting out is in place only when nothing drops to 0.
  - The merge path is correct. Candidates are ascending, the old row is a subset of the candidates, and the merge goes through `Vec::with_capacity(exact)` and `into_boxed_slice`, so it makes no realloc.
  - Counting out a property the row lacks is ignored without a panic.
  - All accesses use `get` or `get_mut`.
- `SetThreatCategory` counts the new category in before the old one out, so shared properties cause no churn.
- Standards:
  - D1 and D2 hold: the arena and typed IDs are untouched.
  - D7 and P4 hold: no hash map; the sorted slices keep the output deterministic.
  - A8 is documented.
  - A9 holds: the per-op cost is bounded by 64 and there is no O(n^2) on an interactive path.
  - R1: there are no needless clones. The `to_vec` and `clone` calls are in tests only.
- Scope: `Arena::slot_count` is now `#[cfg(test)]`, which is a minimal consequence of the change. The flat-index check is removed as the spec settled. No code was added beyond the spec, apart from small helpers (`step`, `row_count`, `new_row_len`) and the extra unit tests.

## Findings (all non-blocking)

- `crates/aegis-model/src/project.rs` (`new_row_len` and `count_row`) -- the exact-size length is computed in a second pass that repeats the merge's own filtering logic. This is correct and tested, so it is not a bug. A simpler alternative would be to collect into a `Vec` and accept a possible shrink realloc, but the exact-size slice is what the spec asked for. No change needed.
- `docs/adr/0010-sparse-reference-counts.md` (Context, last paragraph) -- "Job 011 added three shapes" would read better as "This job adds three shapes". Fix the wording when convenient.
