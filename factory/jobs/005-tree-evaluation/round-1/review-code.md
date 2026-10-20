VERDICT: PASS

Reviewed commit 18be839 on factory/005-tree-evaluation (worktree clean).

## Check results (run from factory/worktrees/005-tree-evaluation)

- `cargo fmt --check` -> pass
- `cargo clippy --all-targets -- -D warnings` -> pass
- `cargo test` -> pass (55 unit, 3 cycles, 10 eval, 11 parents_storage, 2 properties, 45 doc + 1 compile_fail)
- `cargo test --benches` -> pass
- `cargo bench --bench eval` -> pass. Reproduced: large median 2.0 ms (ADR: 1.948 ms), stress 21.1 ms (ADR: 20.56 ms). Budgets are 200 ms and 3 s.
- `scripts/deny.sh licenses bans sources` -> pass (`ok aegis-model`)
- AC13 grep for `unwrap|expect|panic!|[x.index()]` in `src/eval.rs` -> no hits at all
- AC14 ADR 0005 exists with context, options, decision, complexity, shared-subtree limitation, numbers.

## Acceptance criteria

AC1-AC10 are each met by a named integration test in `tests/eval.rs` that I read, and all pass. They assert what the spec asks:
- every node's value in `maxmin_values`;
- SAND = sum and saturation in `min_time_values`;
- linked-child order in `seq_and_order`;
- exactly-once leaf and combine counts on a diamond;
- the 64-level ladder with leaf calls == steps and combines == E - G (258 edges, 129 gates);
- error IDs and Display text with no label;
- a clone gives an equal result;
- 8 fixtures against a naive recursive evaluator for both domains;
- a 100,000-deep chain;
- `get` is `None` for an out-of-range index.

AC11 and AC12 are met. AC15 is met.

## Code review

No blocking findings.

- Logic: `evaluate` walks the reverse of the topological order, and the fold is seeded with one clone and then k-1 combines. Empty gate, missing leaf, and invariant breaks all return typed errors, with no panics. Errors carry `NodeId`, not labels. Stopping at the first error follows the fixed order, so it is deterministic (P4).
- Standards: arena-indexed `Vec` memoization (D1, D2); no recursion; big-O documented on every public item (A8); linear with measured ns/node about constant at 100k and 1M nodes (A9); the accumulator is moved, not cloned (R1). `src/tree.rs` is untouched and there are no new dependencies.
- Simplicity: nothing beyond the spec. `Evaluation` stores the root value in a separate field so `root_value()` has no panic path. This is slightly more complex than a `Vec<V>`, but the ADR justifies it and the spec allowed the choice.

## Non-blocking notes

- `crates/aegis-model/src/eval.rs` (`Evaluation::get`): an `NodeIdx` from another tree that happens to equal this tree's root index returns the root value. The doc says "only meaningful for the tree it was computed from", and the spec only requires `None` for out-of-range indices. No change needed.
- `crates/aegis-model/benches/eval.rs`: `eprintln!` of the fixture summary runs inside the divan table (the builder noted this as cosmetic). It could be moved to a one-time print in `fixture()`.
- Invariant breaks reuse `ModelError::UnknownNode`, since the spec added only two variants. This is acceptable and documented with `INVARIANT` comments.
