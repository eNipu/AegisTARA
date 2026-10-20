## Round 1
Commits: 16ca035 Add property tests for attack tree evaluation on random DAGs
Files: crates/aegis-model/tests/eval_properties.rs (new, 874 lines). Nothing else changed.

Acceptance criteria:
- [x] AC1 — `cargo test --test eval_properties random_dag_matches_naive` → 1 passed (256 cases). Values for `MaxMin<u8>`, `MinTime`, and `Poly` match the naive evaluator on every node, and `eval.len() == tree.len()`.
- [x] AC2 — `cargo test --test eval_properties random_dag_trace_matches_naive` → 1 passed. `eval.derivation(n)` matches the naive derivation on every node for `MaxMin<u8>` and `MinTime` (and also `Poly`, which goes beyond the spec).
- [x] AC3 — `cargo test --test eval_properties random_dag_evaluates_each_node_once` → 2 passed (small: 256 cases, large: 32 cases). `large_random_dag_evaluates_each_node_once` → 1 passed. Checks: leaf closure runs once per step and 0 times per gate; or/and/seq_and counts equal the sum of (children - 1) per kind (`Counting<Poly>` and `Counting<CountedPoly>`); `Counted` clones == number of gates; `CountedPoly` values equal `Poly` values.
- [x] AC4 — `cargo test --test eval_properties random_dag_deterministic` → 2 passed (small and `large_random_dag_deterministic`). For all three domains: evaluating twice, evaluating `tree.clone()`, and evaluating a rebuild from the same plan (root UUID asserted different, child lists asserted equal) give `==` evaluations, equal `{:?}` text, and equal leaf-call sequences.
- [x] AC5 — `cargo test --test eval_properties random_dag_first_error` → 1 passed. The oracle walks `topological_order()` in reverse. Each case checks `fix_empty = false`, and also `fix_empty = true` on the same plan so that the `Ok` and `MissingLeafValue` paths are hit often (beyond the spec). It also checks that `Ok` happens exactly when there is no empty gate and no missing value, and that a second run gives the same result.
- [x] AC6 — `cargo test --test eval_properties generator_coverage -- --nocapture` → pass. Stats on 256 deterministic draws: 222 trees with a node that has 2+ parents (needed ≥ 128); 125 trees with 3+ parents (needed ≥ 1). Non-root OR/AND/SAND gates appear in 239/238/240 trees (needed ≥ 128 each; the root is not counted, which is stricter). 81 trees have depth ≥ 8, 34 trees have no successful link, and 0 trees go over the naive budget (allowed ≤ 12).
- [x] AC7 — `grep -n 'WouldCreateCycle' tests/eval_properties.rs` → line 32 (doc) and line 238 (skip branch in `build`). The skip branch asserts the tree is unchanged: same `len()` and same `children(p)`. `NotAGate` must name `p` and `p` must be a step. Any other error fails the case.
- [x] AC8 — `time cargo test --test eval_properties` → 8 passed in 1.78 s (1.87 s wall), debug build.
- [x] AC9 — `cargo test --test eval --test explain --test properties --test cycles` → 10 + 10 + 2 + 3 passed, unchanged.
- [x] AC10 — `git diff --exit-code 693380c -- src Cargo.toml Cargo.lock` → empty (exit 0). No bug found, so `src/` is unchanged.
- [x] AC11 — The module doc covers the plan format, why the DAGs stay acyclic, the three domains, the naive budget, how to replay a failure, and cost. `grep -cE '\.unwrap\(|\.expect\(' tests/eval_properties.rs` → `0`.
- [x] AC12 — `scripts/deny.sh licenses bans sources` (repo root) → `ok aegis-model`.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (61 unit, 3 cycles, 10 eval, 8 eval_properties, 10 explain, 11 parents_storage, 2 properties, 60 doctests)
- `time cargo test --test eval_properties` → pass, 1.78 s
- `scripts/deny.sh licenses bans sources` → pass

Notes:
- **Failure persistence.** proptest's default `SourceParallel` persistence looks for a `lib.rs` above the test file. It finds none under `tests/`, so it printed "failed to find lib.rs or main.rs" and would not save regressions. `config(cases)` therefore sets `FileFailurePersistence::Direct("proptest-regressions/eval_properties.txt")`, which is relative to the crate root (cargo's test working directory) and is the path the spec asks for. `tests/properties.rs` has the same problem; I left it alone because that file is out of scope.
- **`PROPTEST_CASES`.** `ProptestConfig::with_cases` ignores `PROPTEST_CASES`. `config()` uses 256 or 32 cases only when the variable is unset, so the soak run in the docs (`PROPTEST_CASES=10000 ...`) works. Checked with `PROPTEST_CASES=3`.
- **No regressions file committed.** No property failed on the real engine, so there is no `proptest-regressions/` directory to commit.
- **Mutation sanity check (not committed).** I changed `MinTime::and_kept`/`MaxMin::or_kept` from `>` to `>=` (tie picks the later child). `random_dag_trace_matches_naive` failed and shrank to a 9-op all-OR plan, and the regressions file was written at `crates/aegis-model/proptest-regressions/eval_properties.txt`. I then restored `src/eval.rs` with `git checkout` and deleted the regressions file.
- **Large plans.** Sizes I sampled were 84 to 3,534 nodes, up to 1,883 successful links, and depth up to 200. `link`'s cycle check only walks the descendants of the child, so building them is cheap.
- **Op weights.** Small plans weight Add:Link at 3:2 over 0..=72 ops, capped at 48 Add and 24 Link. Large plans weight them 1:1 over 0..=6000 ops, capped at 3,000 / 3,000. Raw positions are drawn from 0..4096 (small) and 0..2^20 (large) so shrunk plans stay readable.
