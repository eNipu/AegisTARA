# 005-tree-evaluation: Attack tree evaluation over a pluggable attribute domain

## Open questions

1. **Crate.** The backlog says `aegis-model`. `docs/brainstorm.md` (crate table) plans attribute domains in a later `aegis-engine` crate. This spec puts the evaluator in `crates/aegis-model/src/eval.rs` as its own module with no dependency on tree internals beyond the public API, so it can move to `aegis-engine` later without changes. The approver should confirm.
2. **Empty gates.** A gate with no children has no value. Folding from a neutral element would give an empty AND the value "free / trivially feasible", which is the dangerous answer in a security tool. This spec returns `ModelError::EmptyGate(NodeId)` instead, and the domain trait has no neutral elements. Confirm, or ask for neutral elements (then an empty OR would be "infeasible" and an empty AND "trivial").
3. **Missing leaf values.** A step with no caller value is an error (`ModelError::MissingLeafValue(NodeId)`), not a default. Confirm.
4. **Shared subtrees and non-idempotent domains.** For sum-like domains (cost) or probability, bottom-up evaluation on a DAG counts a shared step once per path that reaches it (see the caveat in `docs/brainstorm.md` section 8.3). This job evaluates each node once (A2) and documents this limitation. Exact handling (BDD) is out of scope. Confirm.

## Goal

The engine can compute a value for every node of an attack tree, including DAGs with shared subtrees, in one O(V + E) pass. The caller picks an attribute domain (how OR, AND, and sequential AND combine child values, for example "OR = most feasible child, AND = least feasible child") and supplies the value of each leaf step. Each node is evaluated exactly once, children before parents, in a fixed order, so results are deterministic (P4) and shared subtrees never blow up exponentially (A2). Users see no new UI yet. Explain-why traces and the random-DAG property suite are the next two backlog items and build on this.

## Scope

In:
- Trait `AttributeDomain` with one combine operation per gate kind.
- `evaluate(tree, domain, leaf)`: one pass in reverse topological order, memoized in an arena-indexed `Vec`.
- Result type `Evaluation<V>` (derived data, kept apart from the tree, D10).
- Two built-in domains: `MaxMin<T>` (feasibility: OR = max, AND = min, SAND = min) and `MinTime` (OR = min, AND = max, SAND = saturating sum, so SAND differs from AND).
- New error variants `ModelError::EmptyGate(NodeId)` and `ModelError::MissingLeafValue(NodeId)`.
- Unit tests, doctests, a deep-chain test, an exponential-ladder test, a divan benchmark at `large` and `stress` sizes.
- Decision record `docs/adr/0005-tree-evaluation.md` (L1): trait shape (binary fold vs slice combine vs neutral elements), empty-gate policy, leaf-value callback vs map, benchmark numbers.

Out:
- Explain-why traces (next backlog item). Random-DAG proptest suite against a naive evaluator (the item after that). Keep the design open for both: no change to `Evaluation` should be needed except added fields.
- Incremental recompute (A1), dirty flags, buffer reuse across recomputes.
- Attack potential factor vectors, packs, Pareto sets, probability, BDD/ZDD, top-k paths (A4).
- Countermeasures, `aegis-engine` crate, Cargo workspace, serialization, CI bench gating.

## Design notes

Crate: `crates/aegis-model/` only. Files: new `src/eval.rs`; `src/error.rs` (two variants); `src/lib.rs` (module, re-exports, crate docs pointing to ADR 0005); new `tests/eval.rs`; new `benches/eval.rs` plus a `[[bench]] name = "eval" harness = false` entry in `Cargo.toml`; new `docs/adr/0005-tree-evaluation.md`. No new dependencies (divan is already a dev-dependency). Do not touch `src/tree.rs` except doc links.

**Trait** (static dispatch, D11):
```rust
pub trait AttributeDomain {
    type Value: Clone;
    fn or(&self, acc: Self::Value, next: &Self::Value) -> Self::Value;
    fn and(&self, acc: Self::Value, next: &Self::Value) -> Self::Value;
    /// Defaults to `and`. Called in stored child order, so order-sensitive
    /// domains can rely on it.
    fn seq_and(&self, acc: Self::Value, next: &Self::Value) -> Self::Value {
        self.and(acc, next)
    }
}
```
A gate's value is a left fold over its children in stored order (`tree.children(g)`), seeded with a clone of the first child's value, then `combine(acc, &v_i)` for children 2..k. So a gate with k children costs exactly one clone and k - 1 combine calls. Taking `acc` by value avoids a clone per step of the fold (R1).

**Entry point:**
```rust
pub fn evaluate<D, F>(tree: &AttackTree, domain: &D, leaf: F)
    -> Result<Evaluation<D::Value>, ModelError>
where D: AttributeDomain, F: FnMut(NodeIdx) -> Option<D::Value>;
```
1. `tree.topological_order()?` (parents first; `ModelError::Cycle` passes through, A3).
2. Iterate that order in reverse, so every child is done before its parents. `values: Vec<Option<V>>` with `with_capacity(tree.len())` (R2), indexed by `NodeIdx::index()`.
3. Step: call `leaf(idx)` exactly once; `None` gives `MissingLeafValue(id)`. Gate: no children gives `EmptyGate(id)`; else fold as above, reading memoized child values. A child slot that is still `None` is an invariant break; handle it without panicking (Q3), for example by returning an error, with a comment that it cannot happen after a successful topological order.
4. Stop at the first error in this deterministic order. Errors name nodes by `NodeId` (as the rest of the crate).
- No recursion. Time O(V + E) plus V leaf calls and E combine calls; extra space O(V) for the order and the values. Document both in the doc comment (A8). Cite Mauw and Oostdijk 2005 (attribute domains) and Kahn's order in a comment (A10).

**`Evaluation<V>`**: owns the values in arena order. `get(idx) -> Option<&V>` (O(1), `None` for an index not in the tree), `root(&self) -> &V` or `root_value()` (the tree root's value; store the root index), `len()`, and an iterator `iter() -> impl Iterator<Item = (NodeIdx, &V)>` in arena order. Internal layout (`Vec<Option<V>>` or a checked `Vec<V>`) is the builder's choice; record it in the ADR.

**Built-in domains** (`src/eval.rs`, re-exported):
- `MaxMin<T: Ord + Clone>`: zero-sized marker (`PhantomData`). OR = max, AND = min, SAND = min. Doc example: feasibility levels as an ordered enum or `u8`.
- `MinTime`: `Value = u64`. OR = min, AND = max (parallel), SAND = `saturating_add` (sequential). No overflow panic.

**Benchmark** `benches/eval.rs` (divan): build a tree through the public API outside the timed region, at 100,000 (`large`) and 1,000,000 (`stress`) nodes, with a few percent of nodes shared via `link`, mixed gate kinds, and `MaxMin<u8>` leaf values from a slice. Time `evaluate` only. The budget to compare against: full recompute ≤ 200 ms at `large`, ≤ 3 s at `stress` (engineering standards section 2). Record the numbers and machine in ADR 0005. CI gating stays out of scope.

Project-sized operations: `evaluate` O(V + E) time, O(V) space. Nothing O(V^2) (A9).

## Acceptance criteria

All commands run from `crates/aegis-model/`.

- [ ] AC1 OR, AND, and SAND compute the right values with `MaxMin<u8>` on a hand-built three-level tree (root OR over an AND and a SAND, each over steps); every node's value is checked, not only the root. Test: `cargo test --test eval maxmin_values`.
- [ ] AC2 `MinTime` gives SAND = sum and AND = max on the same children (proves `seq_and` is dispatched for `GateKind::SeqAnd`), and SAND saturates at `u64::MAX` instead of panicking. Test: `cargo test --test eval min_time_values`.
- [ ] AC3 Fold order: a test-only domain whose `seq_and` concatenates strings returns the children's values in stored child order, including a child added by `link`. Test: `cargo test --test eval seq_and_order`.
- [ ] AC4 Exactly once: on a diamond (one step shared by two gates via `link`), with a counting domain and a counting leaf closure, the leaf closure runs exactly once per step, the total number of combine calls equals the sum over gates of (children - 1), and the shared step's value is used by both gates. Test: `cargo test --test eval evaluates_each_node_once`.
- [ ] AC5 No exponential blow-up: a ladder DAG of 64 levels, two gates per level, each gate linked to both gates of the next level (2^64 root-to-leaf paths), evaluates with leaf calls == number of steps and combine calls == E - (number of gates), and finishes in the normal test run. Test: `cargo test --test eval ladder_is_linear`.
- [ ] AC6 Errors: an empty gate gives `Err(ModelError::EmptyGate(id))` with that gate's `NodeId`; a leaf closure returning `None` gives `Err(ModelError::MissingLeafValue(id))` with that step's `NodeId`; the `Display` text of both names the ID and no label. Test: `cargo test --test eval eval_errors`.
- [ ] AC7 Determinism: evaluating the same tree twice, and a `clone()` of it, gives equal results (`Evaluation<V>: PartialEq + Debug` for `V: PartialEq + Debug`). Test: `cargo test --test eval eval_deterministic`.
- [ ] AC8 Agreement with a naive recursive evaluator written in the test file, on at least 5 fixed trees and DAGs (with shared nodes and all three gate kinds), for both built-in domains, on every node. Test: `cargo test --test eval matches_naive`.
- [ ] AC9 No recursion: a chain of 100,000 nested gates over one step evaluates without stack overflow on the default test thread. Test: `cargo test --test eval deep_chain_eval`.
- [ ] AC10 `Evaluation::get` returns `None` for an index from another, larger tree, and `root_value()` equals `get(tree.root())`. Test: `cargo test --test eval evaluation_accessors`.
- [ ] AC11 Benchmark runs at both sizes, and its smoke run passes: `cargo test --benches` and `cargo bench --bench eval`. ADR 0005 reports the `large` and `stress` medians against the 200 ms / 3 s budgets.
- [ ] AC12 Every new public item has a doc comment with a compiling example and states its time and space cost (Q5, A8). Check: `cargo test --doc` and `cargo clippy --all-targets -- -D warnings` pass (crate has `missing_docs` and `clippy::pedantic`).
- [ ] AC13 No `unwrap`, `expect`, `panic!`, or panicking indexing in `src/eval.rs` outside `#[cfg(test)]`. Check: `grep -nE '\.unwrap\(|\.expect\(|panic!|\[[a-z_]+\.index\(\)\]' src/eval.rs` shows hits only inside test modules.
- [ ] AC14 `docs/adr/0005-tree-evaluation.md` exists with context, options, decision, complexity, the shared-subtree limitation from Open question 4, and the bench numbers. Check: `test -f ../../docs/adr/0005-tree-evaluation.md`.
- [ ] AC15 License policy still passes. Check (from the repo root): `scripts/deny.sh licenses bans sources`.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --benches
cargo bench --bench eval
```

From the repo root:

```sh
scripts/deny.sh licenses bans sources
```
