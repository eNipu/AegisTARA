VERDICT: PASS

## Check results (run in factory/worktrees/007-eval-property-tests)

- `cargo fmt --check` -> pass
- `cargo clippy --all-targets -- -D warnings` -> pass
- `cargo test` -> pass (61 unit, 3 cycles, 10 eval, 8 eval_properties, 10 explain, 11 parents_storage, 2 properties, 60 doctests, 1 other doctest group)
- `time cargo test --test eval_properties` -> pass, 8 tests, 1.7 s wall (limit 60 s)
- `scripts/deny.sh licenses bans sources` -> pass (`ok aegis-model`)
- `git diff 693380c -- crates/aegis-model/src crates/aegis-model/Cargo.toml Cargo.lock` -> empty (AC10)
- `grep -cE '\.unwrap\(|\.expect\(' tests/eval_properties.rs` -> 0 (AC11)
- `grep -n WouldCreateCycle tests/eval_properties.rs` -> line 238 is the skip branch (AC7)
- Worktree clean; the diff is only `crates/aegis-model/tests/eval_properties.rs` (new). No regressions file, which matches build.md (no failure found).

## Acceptance criteria

AC1 to AC12 are met and tested.
- AC1/AC2: the naive evaluator is written independently (recursion, no memo, own selection rules). It compares values and derivations on every node for MaxMin<u8>, MinTime and Poly. `eval.len() == tree.len()` is asserted.
- AC3: the leaf closure count per node index, the per-kind or/and/seq_and sums, and the `Counted` clone count equal the gate count. Small and large variants exist.
- AC4: twice, clone, and a rebuild with new UUIDs. The rebuild is checked to have a different root id and the same child lists. Equality, `Debug` text, and the leaf-call sequence are all compared.
- AC5: the oracle walks `topological_order()` reversed. The `Ok` and `MissingLeafValue` paths are also covered via `fix_empty = true`.
- AC6: the thresholds are asserted with `TestRunner::deterministic()`. The kind-coverage check excludes the root, which is stricter than the spec.
- Module docs cover plan format, acyclicity, domains, budget, replay, and cost (A8).

## Findings

No blocking findings. Minor, no action required:

- tests/eval_properties.rs:725 — `config()` sets `FileFailurePersistence::Direct(...)` so regressions land at the path the spec names. This is justified in the doc comment and build.md. `tests/properties.rs` has the same lib.rs-lookup problem. It is out of scope, so consider a later backlog item.
- tests/eval_properties.rs:775 — `random_dag_first_error` also runs the `fix_empty = true` variant. This goes beyond the spec but is small and makes the `Ok` and missing-value paths reachable often. Acceptable.
- tests/eval_properties.rs:597 — `CLONES` is thread-local and reset before each use, which is safe because proptest runs each case on the test thread.
