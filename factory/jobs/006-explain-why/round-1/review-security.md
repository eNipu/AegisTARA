VERDICT: PASS

Scope: `git diff master...factory/006-explain-why` (eval.rs, lib.rs, tests/explain.rs, benches/eval.rs header comment, ADR 0006). This is round 1, so there are no earlier security reviews to check against.

Findings: none blocking.

- Input at the boundary (Q4): `derivation`, `explain`, and `why_path` take caller-supplied `NodeIdx` and possibly a different tree. All reads use `.get()`/`get_mut()` and return `Option`. `explain` rejects a length mismatch and a node kind or gate kind mismatch before it borrows child lists, and `inputs()` uses `filter_map` over `eval.get`, so no input can cause a panic.
- Bounds (R3/R4/R6): `why_path` is a lazy `successors(...).take(self.len())`. A corrupted or cyclic trace cannot loop forever, and it allocates nothing. `derivations` is preallocated with `with_capacity(tree.len())` and grows with the tree. It adds no queue, recursion, or unbounded growth.
- No `unsafe` (Q2). No `unwrap`/`expect`/`panic!` added in non-test code (Q3). The test code uses only `unwrap_or`.
- No hash maps added (D7 not affected). No new dependencies and no Cargo changes (D5).
- No secrets, tokens, or customer data in code, tests, or the ADR. `Explanation`'s hand-written `Debug` prints only the node, the value, the derivation, and the child indices.
- Non-blocking, already documented: `explain` cannot detect a same-count, same-kind edit (for example a new `link`). In that case it returns stale but memory-safe data, and this limit is stated in the doc comment and in ADR 0006.
