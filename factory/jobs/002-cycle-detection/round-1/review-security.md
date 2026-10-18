VERDICT: PASS

This diff adds no external input boundary: no file, network, or plugin I/O, and no deserialization. It only adds in-memory graph operations in `crates/aegis-model`. The items below were checked and found clean.

- No `unsafe` code (Q2). No `unwrap`, `expect`, `panic!`, or slice indexing in non-test code in `src/topo.rs`, `src/error.rs`, or the new `link` and `topological_order` in `src/tree.rs`. All lookups use `get` or `get_mut` (Q3).
- `link` validates both indices before it changes anything. A child index that is out of range returns `UnknownNode` and is never stored. `topo.rs` also guards every `in_degree`, `pred`, and `visit_pos` access with `get`, so a bad index cannot panic.
- `topo.rs:23-33`: in-degree uses `saturating_add` and `saturating_sub`, so it cannot overflow.
- `topo.rs:36-56`: the `VecDeque` is a work queue local to the algorithm, not a channel. Each node is pushed at most once, because its in-degree reaches 0 only once, so the queue is bounded by V (R4 does not apply).
- The cycle walk in `topo.rs:107-122` stops after at most V steps, because a node that repeats ends it. The algorithm has no recursion, and `deep_chain` tests 100,000 nodes, so a stack overflow is not possible.
- The `Cycle` path and its `Display` string grow with the cycle length, so at most O(V). That is acceptable for an in-memory model.
- The diff adds no hash maps (D7 does not apply) and no new dependencies (no `Cargo.toml` change, so D5 does not apply).
- No secrets, tokens, or customer data in the code, tests, or ADR.

Note for later, not a finding: `link` is O(k) per call, and the number of edges can grow as O(V^2). When an import path is added later, it should cap node and edge counts at the boundary (Q4, R3, R6) so a hostile file cannot cause quadratic work.
