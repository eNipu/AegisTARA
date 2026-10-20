# 007-eval-property-tests: Property tests for attack tree evaluation on random DAGs

## Open questions

1. **Tests only, no ADR.** This job adds tests, not product code or a new design choice, so it writes no ADR. The generator design (how random DAGs are built and why they cover the cases that matter) goes in the module doc comment of the new test file. Confirm, or ask for a short `docs/adr/0007-eval-property-tests.md`.
2. **Production code changes.** `src/` should not change. If a property finds a real bug, the builder fixes it with the smallest change in `src/eval.rs`, adds a fixed regression test to `tests/eval.rs` or `tests/explain.rs`, and explains it in `build.md`. Confirm that fixing in this job is fine (the alternative is to stop and open a new backlog item).
3. **Case counts and run time.** 256 cases for small DAGs and 32 for large DAGs, so the suite runs in under 60 s in a debug build. A long soak (for example `PROPTEST_CASES=10000`) stays a manual or later CI job. Confirm.
4. **Regression files.** If proptest finds a failure while the builder works, it writes `crates/aegis-model/proptest-regressions/eval_properties.txt`. This spec commits that file (the proptest recommendation), so the failing seed is replayed on every run. Confirm.

## Goal

The evaluation engine (jobs 005 and 006) is checked on thousands of random attack trees, not only on hand-built fixtures. Random DAGs with shared subtrees, all three gate kinds, and random leaf values show three things on every run: evaluation is deterministic (P4: same tree, same result, byte for byte, also for a clone and for the same tree rebuilt with new UUIDs); each node is evaluated exactly once (A2: each step's leaf value is asked once, each gate folds its children once); and every node's value and explain-why trace agree with a simple recursive evaluator written in the test. Users see no change. Developers get a safety net for later engine work (incremental recompute, new domains).

## Scope

In:
- New test file `crates/aegis-model/tests/eval_properties.rs` with proptest strategies for random build plans, a plan-to-tree builder, a naive recursive evaluator (values and traces), counting wrappers, and the properties below.
- Three domains in the test: `MaxMin<u8>`, `MinTime`, and a test-only order-sensitive domain `Poly` whose three gate rules differ, so child order or gate kind mix-ups change the result.
- A deterministic coverage test that proves the generator really produces shared nodes, all gate kinds, deep trees, and trees with no links.
- An error property: on trees with empty gates or missing leaf values, `evaluate` returns the first error in reverse topological order.

Out:
- Changes to `src/` (see Open question 2), new dependencies, an ADR (Open question 1).
- Property tests for `link`/`add_child` invariants (already in `tests/properties.rs`) and cycle detection (`tests/cycles.rs`).
- Refactoring helpers in `tests/eval.rs` and `tests/explain.rs`. A shared `tests/common/` module is allowed but not required.
- Fuzzing, mutation testing, CI soak runs, benchmarks.
- Monotonicity properties (for example "an easier OR child never makes the parent harder"); a later engine job.

## Design notes

Crate: `crates/aegis-model/` only. One new file: `tests/eval_properties.rs`. `proptest = "1"` is already a dev-dependency; `Cargo.toml` and `Cargo.lock` must not change. Use only the public API.

**Build plan (what proptest generates and shrinks).**
```rust
#[derive(Clone, Debug)]
enum Op {
    Add { parent: usize, kind: u8 },   // kind 0..3 = Or/And/SeqAnd, 3 = step
    Link { parent: usize, child: usize },
}
struct Plan { ops: Vec<Op>, leaves: Vec<u64> }  // leaves: values by node index
```
- `build(plan, fix_empty: bool) -> AttackTree`: start with an OR root. `Add`: parent = node `parent % len`; if it is a step, use its owner instead (keeps the op useful); add a new node with a fresh `NodeId::new_v7()`. `Link`: `p = parent % len`, `c = child % len`; call `tree.link(p, c)`. `NotAGate`, `DuplicateEdge`, and `WouldCreateCycle` mean "skip" (assert `len()` is unchanged); any other error fails the case. Links may go in either index direction, so the DAGs are not limited to "low index to high index" shapes, and the cycle check of `link` keeps them acyclic.
- `fix_empty = true`: afterwards add one step child to every gate with no children (collect them first, O(V)), so `evaluate` succeeds. The error property uses `fix_empty = false`.
- Leaf value of step `i` is `leaves[i % leaves.len()]`, mapped per domain: `MaxMin<u8>` uses `v % 4` (few levels, many ties, which tests the tie rule); `MinTime` uses the raw value, drawn from a mix of small values and values near `u64::MAX` (tests saturation); `Poly` uses the raw value.
- Strategy sizes: small plans 0..=48 `Add` and 0..=24 `Link` ops, interleaved (links happen while the tree grows); large plans up to 3,000 `Add` and 3,000 `Link` ops. Weight `Link` so most small plans share at least one node. Shrinking works on `ops`, so a failure shrinks to a small plan; the `Debug` output of the plan must be enough to rebuild the tree by hand.
- `link` costs O(V + E) per call (cycle check), so a large plan costs at most about 3,000 x 6,000 steps per case. Fine for 32 cases.

**Domains.** `MaxMin<u8>` and `MinTime` from the crate. `Poly` (test-only, `Value = u64`): OR `acc.wrapping_mul(31).wrapping_add(next ^ 1)`, AND `acc.wrapping_mul(37).wrapping_add(next ^ 2)`, SAND `acc.wrapping_mul(41).wrapping_add(next ^ 3)`. Not commutative, so a fold in the wrong order or with the wrong gate rule gives a different value.

**Naive evaluator.** Plain recursion from a node through `tree.children(n)` in stored order, no memo, no topological order. Returns the value and the `Derivation` for each node, computed independently of `evaluate` (selection rules: first child, then a strictly better later child replaces it; `MinTime` SAND and every `Poly` gate are `Combined`; one-child gates are `Selected`). It is called once per node. It is exponential on DAGs on purpose, so it has a call budget (1,000,000 recursive calls per case); a case over budget is rejected with `prop_assume!`, and the coverage test asserts this is rare. Recursion depth is at most the node count (≤ 73 for small plans), so the stack is safe.

**Counting.** `Counting<D>` wraps a domain and counts plain `or`/`and`/`seq_and` calls per gate kind (the default `*_kept` methods call the plain method exactly once, see 006). The leaf closure counts calls per node index in a `Vec<u32>` and records the call sequence. A test value type `Counted(u64)` whose `Clone` increments a shared counter (`Rc<Cell<usize>>` or a `thread_local!`) counts clones; its domain never clones in `or`/`and`/`seq_and` (it reads `next.0`).

**Cost.** Test code only. The plan builder is O(ops x (V + E)) because of `link`; the naive evaluator is exponential and bounded by its budget; all other checks are O(V + E) per case. Nothing in this job runs in production.

## Acceptance criteria

All commands run from `crates/aegis-model/` unless noted.

- [ ] AC1 Agreement on values: for random small DAGs (`fix_empty = true`), `evaluate` and the naive evaluator give the same value for every node, for `MaxMin<u8>`, `MinTime`, and `Poly`, and `eval.len() == tree.len()`. Test: `cargo test --test eval_properties random_dag_matches_naive`.
- [ ] AC2 Agreement on traces: for the same DAGs, `eval.derivation(i)` equals the naive derivation for every node, for `MaxMin<u8>` (ties pick the earlier child) and `MinTime` (SAND is `Combined`). Test: `cargo test --test eval_properties random_dag_trace_matches_naive`.
- [ ] AC3 Exactly once: for random small and large DAGs, the leaf closure runs exactly once for every step and never for a gate; the `or`, `and`, and `seq_and` call counts each equal the sum of (children - 1) over the gates of that kind; with `Counted`, the number of value clones equals the number of gates. Tests: `cargo test --test eval_properties random_dag_evaluates_each_node_once` and `cargo test --test eval_properties large_random_dag_evaluates_each_node_once`.
- [ ] AC4 Determinism (P4): for random small and large DAGs, evaluating the tree twice, evaluating `tree.clone()`, and evaluating a tree rebuilt from the same plan (new UUIDs) all give `==` `Evaluation`s with equal `format!("{eval:?}")` strings, and the leaf-call sequences are identical. Test: `cargo test --test eval_properties random_dag_deterministic`.
- [ ] AC5 Errors: for random small DAGs with `fix_empty = false` and a random subset of steps whose leaf value is `None`, `evaluate` returns `Ok` exactly when there is no empty gate and no missing value; otherwise it returns `EmptyGate(id)` or `MissingLeafValue(id)` for the first offending node in `tree.topological_order()` reversed (the oracle is computed in the test), and the same error twice in a row. Test: `cargo test --test eval_properties random_dag_first_error`.
- [ ] AC6 Generator coverage: drawing 256 small plans with `proptest::test_runner::TestRunner::deterministic()` gives: at least 50% of trees with a node that has two or more parents, at least one tree with a node that has three or more parents, every gate kind in at least 50% of trees, at least one tree of depth 8 or more, at least one tree with no successful link, and at most 5% of trees over the naive call budget. Test: `cargo test --test eval_properties generator_coverage`.
- [ ] AC7 The plan builder never sees an unexpected `link` error, and skipped links leave the tree unchanged (asserted inside `build`, so AC1 to AC6 cover it). Check: `grep -n 'WouldCreateCycle' tests/eval_properties.rs` shows the skip branch.
- [ ] AC8 Run time: the whole file runs in under 60 s in a debug build. Check: `time cargo test --test eval_properties`.
- [ ] AC9 Existing tests still pass unchanged, including the exact call counts of 005 and the traces of 006. Check: `cargo test --test eval --test explain --test properties --test cycles`.
- [ ] AC10 No production or dependency change, unless Open question 2 applies: `git diff --exit-code <base> -- src Cargo.toml Cargo.lock` is empty, where `<base>` is the job's base commit. If `src/` had to change, `build.md` names the bug, the fix, and the new regression test.
- [ ] AC11 The test file has a module doc comment that explains the plan format, why the DAGs stay acyclic, the three domains, the naive budget, and how to replay a failure. No `unwrap` or `expect` outside proptest helpers that return `TestCaseError` (match the style of `tests/properties.rs`). Check: `grep -cE '\.unwrap\(|\.expect\(' tests/eval_properties.rs` prints `0`.
- [ ] AC12 License policy still passes. Check (from the repo root): `scripts/deny.sh licenses bans sources`.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
time cargo test --test eval_properties
```

From the repo root:

```sh
scripts/deny.sh licenses bans sources
```
