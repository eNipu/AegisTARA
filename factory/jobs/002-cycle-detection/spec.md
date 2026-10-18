# 002-cycle-detection: DAG links and Kahn cycle detection in aegis-model

## Open questions

1. **Are cycles allowed in the model?** Job 001 left `link(parent, child)` (shared subtrees, DAG edges) to this job. Without it, a cycle cannot exist and there is nothing to detect. This spec makes `link` **permissive**: it accepts an edge even if it closes a cycle. Cycles are caught by `topological_order()` before evaluation, as A3 says. This matches later data sources (file import, merges) that can also contain cycles. The other option is to reject a cycle-closing edge in `link` (O(V + E) reachability check per call). The approver should confirm the permissive choice. If eager rejection is wanted, AC3 changes and AC5-AC8 still apply.
2. **Error payload.** The cycle path uses `NodeIdx` (internal), like the existing `UnknownNode(NodeIdx)`. Callers turn it into `NodeId`s with `id_of`. Confirm, or ask for `NodeId`s in the error.
3. **`parent()` with shared nodes.** After `link`, a node can have more than one parent. This spec keeps `parent()` as the *owning* parent (the one from `add_child`) and adds no multi-parent query. Confirm.

## Goal

An attack tree can share a subtree between gates (for example, one "obtain key fob" step under two OR gates). It then becomes a DAG, so it can contain a cycle by mistake. Before evaluation, the model gives a topological order of all nodes in O(V + E) with Kahn's algorithm. If there is a cycle, it returns a typed error that names the nodes on one cycle, in edge order, so the user sees which gates loop and the engine never recurses forever (standards A2, A3). Users see no new UI yet. The engine work will call this before scoring.

## Scope

In:
- `AttackTree::link(parent, child)`: adds an existing node as an extra child of a gate.
- `AttackTree::topological_order()`: Kahn's algorithm, parents before children, deterministic.
- New error variants `ModelError::Cycle` and `ModelError::DuplicateEdge`.
- Unit tests, property tests, doctests, a deep-chain test (no recursion, no stack overflow).
- Short decision record `docs/adr/0002-dag-links-and-cycle-detection.md` (L1): permissive `link` vs eager rejection, Kahn vs Tarjan, how the cycle path is extracted.
- Updating doc comments in `tree.rs`, `lib.rs`, and ADR 0001 that say "never has cycles".

Out:
- Removing links or nodes, moving nodes, undo.
- Multi-parent queries (`parents_of`), Tarjan SCC (all cycles), reporting more than one cycle.
- Incremental or cached topological order (A1). Evaluation and scoring.
- Benchmarks (no bench setup yet), serialization, Cargo workspace.

## Design notes

Crate: `crates/aegis-model/` only. Files: `src/tree.rs`, `src/error.rs`, `src/lib.rs` (docs and re-exports), new `src/topo.rs` (algorithm, private module), `tests/properties.rs` or a new `tests/cycles.rs`, `docs/adr/0002-...md`, `docs/adr/0001-...md` (one line pointing to 0002).

**`link`**
```rust
pub fn link(&mut self, parent: NodeIdx, child: NodeIdx) -> Result<(), ModelError>
```
- Appends `child` to `parent`'s children. Does **not** change `parents[child]` (the owning parent stays).
- Errors, checked in this order, tree unchanged on error: `UnknownNode(parent)`, `UnknownNode(child)`, `NotAGate(parent)`, `DuplicateEdge { parent, child }` if `child` is already in `parent`'s children.
- Does not check for cycles (Open question 1). Self-links (`parent == child`) are accepted and form a one-node cycle.
- Time O(k), k = number of children of `parent` (duplicate scan). Space O(1) amortized. Document both (A8).

**`topological_order`**
```rust
pub fn topological_order(&self) -> Result<Vec<NodeIdx>, ModelError>
```
- Kahn's algorithm (Kahn 1962, cite in a comment, A10). In-degree array `Vec<u32>` over all child edges. Queue (`VecDeque<NodeIdx>`) seeded with in-degree-0 nodes in ascending index; children are visited in their stored order. No recursion.
- Ok: every node exactly once, and for every edge `p -> c`, `p` comes before `c`. Deterministic: the same tree gives the same order (P4).
- Err: if fewer than `len()` nodes were output, the rest contain at least one cycle. Extract one cycle in O(V + E):
  1. For each remaining node, record one remaining predecessor: scan parents in ascending index, children in stored order, keep the first hit. Every remaining node has one, because its in-degree is still above 0.
  2. Start at the smallest remaining index and walk predecessors, marking visit positions (`Vec<u32>` or bitset, D8), until a node repeats. The nodes from the first visit of that node to the repeat form a cycle.
  3. Reverse it into edge direction (parent -> child) and rotate so the smallest `NodeIdx` is first.
- Return `ModelError::Cycle(path)` where `path: Vec<NodeIdx>` (or `Box<[NodeIdx]>`), `path.len() >= 1`, no repeats, `path[i] -> path[i+1]` is an edge for every i, and `path[last] -> path[0]` is an edge.
- Display: `cycle in attack tree: 3 -> 7 -> 4 -> 3` (indices, first node repeated at the end only in the text).
- Time O(V + E), extra space O(V) plus the output. Document this in the doc comment (A8). No new dependencies, except `fixedbitset` (MIT/Apache) if used for D8.

**`ModelError` additions** (enum is `#[non_exhaustive]`, so adding is fine):
- `Cycle(Vec<NodeIdx>)` (or a named field `path`).
- `DuplicateEdge { parent: NodeIdx, child: NodeIdx }`.

Invariant changes to write into the `AttackTree` doc: every non-root node has exactly one *owning* parent and appears in its children; it may also appear in other gates' children through `link`, at most once per gate. The structure is a directed graph that may contain cycles; call `topological_order()` before evaluating.

Project-sized operations: `topological_order` O(V + E) time, O(V) space. `link` O(children of parent). Nothing O(V^2) (A9).

## Acceptance criteria

All test commands run from `crates/aegis-model/`.

- [ ] AC1 `link` adds a shared child: after `link(g2, s)` where `s` was added under `g1`, `children(g2)` ends with `s`, `children(g1)` still has `s`, and `parent(s) == Some(g1)`. Test: `cargo test link_shares_child`.
- [ ] AC2 `link` errors in the documented order and leaves the tree unchanged (compare `len`, `children` of every node before and after): unknown parent, unknown child, step parent (`NotAGate`), repeated edge (`DuplicateEdge`). Test: `cargo test link_errors`.
- [ ] AC3 `link` accepts a cycle-closing edge and a self-link (returns `Ok(())`). Test: `cargo test link_allows_cycle`.
- [ ] AC4 On a tree with no cycles (including shared children), `topological_order()` returns every node once, parents before children, root first when the root has no incoming link. Test: `cargo test topo_order_acyclic`.
- [ ] AC5 Self-link `a -> a` gives `Err(ModelError::Cycle(vec![a]))`. Test: `cargo test cycle_self_link`.
- [ ] AC6 A known cycle `root -> g1 -> g2 -> g3 -> g1` (via `link(g3, g1)`) plus extra nodes hanging below the cycle gives exactly `Cycle(vec![g1, g2, g3])` (smallest index first, edge order, nodes below the cycle not included). Test: `cargo test cycle_path_exact`.
- [ ] AC7 Determinism: calling `topological_order()` twice, and on a `clone()` of the tree, gives equal results (Ok or Err). Test: `cargo test topo_deterministic`.
- [ ] AC8 Property test (proptest, at least 32 cases, up to 500 nodes, up to 500 random links): `topological_order()` is `Err` exactly when a simple reference cycle check written in the test finds a cycle. On `Ok`, the order is a permutation of all indices and respects every edge. On `Err(Cycle(p))`, `p` is non-empty, has no repeats, every consecutive pair and the closing pair `p[last] -> p[0]` are edges, and `p[0]` is the smallest index in `p`. Test: `cargo test --test properties` (or `--test cycles`).
- [ ] AC9 No recursion: a chain of 100,000 gates (each the child of the previous) returns `Ok` with the chain in order, and the same chain with `link(last, root)` returns `Cycle` of length 100,000, both without stack overflow on the default test thread. Test: `cargo test deep_chain`.
- [ ] AC10 `Display` of `Cycle` matches `cycle in attack tree: <i0> -> <i1> -> ... -> <i0>`. Test: `cargo test cycle_display`.
- [ ] AC11 Every new public item has a doc comment with a compiling example and states time and space cost (Q5, A8). Check: `cargo test --doc` passes and `cargo clippy --all-targets -- -D warnings` (crate has `missing_docs` and `clippy::pedantic` on) passes.
- [ ] AC12 No `unwrap`, `expect`, `panic!`, or panicking slice indexing in `src/` outside `#[cfg(test)]`. Check: `grep -nE '\.unwrap\(|\.expect\(|panic!' src/*.rs` shows hits only inside test modules.
- [ ] AC13 Docs updated: `grep -n "never has cycles" src/ ../../docs/adr/0001-arena-and-typed-ids.md` finds nothing outdated, and `docs/adr/0002-dag-links-and-cycle-detection.md` exists with context, options, decision, and complexity.
- [ ] AC14 `spikes/`, `docs/decisions.md`, and `docs/engineering-standards.md` are unchanged. Check: `git diff --stat master -- spikes docs/decisions.md docs/engineering-standards.md` is empty.

## Checks

From `/Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model`:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
