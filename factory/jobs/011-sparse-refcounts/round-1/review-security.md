VERDICT: PASS

Scope reviewed: `git diff master...factory/011-sparse-refcounts` (src/project.rs, src/store/snapshot.rs doc only, benches/store_memory.rs, ADRs 0008/0009/0010). First security review for this job; no earlier rounds.

Untrusted-input paths touched: `Project::load` and op-log replay both drive `RefCounts` updates through `apply`. Checked:

- Q4 boundary validation: unchanged. `check_add_threat_scenario` still requires `violates(category) ⊆ asset.properties` (re-asserted in `check_invariants`, project.rs:1562), so a row can never hold more entries than its asset's file record lists. A hostile file cannot get a 64-entry (512 B) row without paying for 64 property keys in the asset record. The worst shape (`targeted_wide_max`) is measured at 1.36x kept, under the 3x budget.
- R3/R4 bounds: each row holds at most 64 entries (8 B each). Every `count_row` call is O(r + p) with r, p <= 64, and it makes at most one exact-size allocation that replaces the old box, so there is no net growth. Op-log churn (add/remove or recategorize repeatedly) costs O(64) and one bounded allocation per op. No queues or unbounded collections were added.
- R6 memory gates: the bench now gates kept at `SERVER_BUDGET = 3.0` for every loading case, rejects any kept gate set above that budget, and lowers or keeps every peak gate (`assets_only_max` 8.3 -> 3.3). The new `targeted_narrow_max` shape found a 3.60x kept breach on master, and this job fixes it (1.76x).
- Q3 no panics: the new library code (`count_row`, `new_row_len`, `row_count`, `RefCounts::{threat, properties, property}`, `commit_add_asset`) uses `get`/`get_mut`, `binary_search_by_key(...).ok().and_then(get)`, `next_if`, and saturating `step`. It does no indexing and calls no `unwrap`/`expect`. The only `+= 1`, indexing and `expect` calls are in `#[cfg(test)]` code (`check_invariants`, tests).
- Removing the flat-index overflow check in `check_add_asset` is safe because nothing is flat-indexed any more. The slot-limit check (`next_idx(self.slot_limit)`) still bounds the asset count.
- Q2: no `unsafe`. `forbid(unsafe_code)` is unchanged.
- D7: no new hash maps. The test-only recount uses `BTreeMap`.
- D5: no new dependencies. Cargo.toml and Cargo.lock are unchanged.
- No secrets, tokens or customer data in code, tests, benches or docs. Bench inputs are synthetic.

Findings: none.
