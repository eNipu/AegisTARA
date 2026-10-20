# ADR 0005: Attack tree evaluation over a pluggable attribute domain

- Status: proposed (job 005-tree-evaluation)
- Date: 2026-10-20
- Crate: `crates/aegis-model` (`src/eval.rs`; uses only the public tree API)
- Rules: P1, P3, P4, D10, D11, A2, A3, A8, A9, A10, R1, R2, Q3, Q5, L1, L2 in
  `docs/engineering-standards.md`
- Builds on: `0002-dag-links-and-cycle-detection.md` (topological order)

## Context

The engine must compute a value for every node of an attack tree: a
feasibility level, a minimum time, later attack potential and more. How a
gate combines its children differs per attribute and per methodology
(`docs/brainstorm.md` section 8.3), so the math must be pluggable. Attack
tree research calls such a choice an **attribute domain** (Mauw and
Oostdijk, "Foundations of Attack Trees", ICISC 2005): one operation per
gate kind.

Trees are DAGs since ADR 0002 (`AttackTree::link` shares subtrees). Naive
recursion visits a shared node once per path, which can be exponential
(rule A2). Results must be deterministic (P4), and errors must name nodes by
`NodeId`, never by label.

The owner confirmed the spec's open questions on 2026-10-20 (`rework.md`):
evaluator in `aegis-model` for now, empty gates and missing leaf values are
errors, and shared subtrees are evaluated once with the limitation
documented below.

## Options

### Trait shape

| Option | Good | Bad |
|---|---|---|
| **A. Binary fold, no neutral element** (`or(acc, &next) -> acc`, per kind) | Smallest trait; one clone per gate, accumulator moves (R1); fold order is defined, so order-sensitive SAND domains work; works for types with no natural identity (ordered enums) | Empty gates need a separate rule |
| B. Slice combine (`or(&[&V]) -> V`) | Domain sees all children at once (useful for Pareto sets, top-k) | Needs a temporary `Vec<&V>` per gate (one allocation per gate in the hot path, or a reused buffer threaded through) |
| C. Monoid with neutral elements (`zero_or`, `zero_and`) | Fold from the identity; empty gates "just work" | Empty AND = identity = "trivially feasible", the dangerous answer in a security tool; every domain must invent identities (what is the identity of an enum level?) |

**Chosen: A.** B can be added later as a provided method with the fold as
default if Pareto domains need it. C is rejected for safety (P1).

### Empty gates

Return `ModelError::EmptyGate(NodeId)` (chosen) versus fold from a neutral
element (option C above). An empty gate in a real model is an unfinished
edit; reporting it is better than a silent "free attack".

### Leaf values

| Option | Good | Bad |
|---|---|---|
| **Callback `FnMut(NodeIdx) -> Option<V>`** | No allocation; caller can read a slice, a struct-of-arrays column (D4), or compute on the fly; called exactly once per step | Caller must map `NodeIdx` |
| Map `&HashMap<NodeIdx, V>` or `&[V]` | Simple | Forces one storage layout on every caller; a map costs a hash per step |

**Chosen: callback.** A missing value is `ModelError::MissingLeafValue(NodeId)`,
never a default. A slice is one closure away: `|i| levels.get(i.index()).copied()`.

### Static versus dynamic dispatch

`evaluate<D: AttributeDomain>` is generic (D11), so the combine calls inline
into the fold. Packs that pick a domain at run time can wrap a `dyn` or an
`enum` domain at the boundary.

## Decision

```rust
pub trait AttributeDomain {
    type Value: Clone;
    fn or(&self, acc: Self::Value, next: &Self::Value) -> Self::Value;
    fn and(&self, acc: Self::Value, next: &Self::Value) -> Self::Value;
    fn seq_and(&self, acc: Self::Value, next: &Self::Value) -> Self::Value { self.and(acc, next) }
}

pub fn evaluate<D, F>(tree: &AttackTree, domain: &D, leaf: F)
    -> Result<Evaluation<D::Value>, ModelError>
where D: AttributeDomain, F: FnMut(NodeIdx) -> Option<D::Value>;
```

Algorithm (memoized bottom-up evaluation, no recursion):

1. `tree.topological_order()?`: Kahn's algorithm, parents first. A cycle
   passes through as `ModelError::Cycle` (A3).
2. Walk the order in reverse, so every child is done before its parents.
   Values live in `Vec<Option<V>>` (`with_capacity(tree.len())`, R2),
   indexed by `NodeIdx::index()`.
3. Step: call `leaf(idx)` once. Gate: left fold over `tree.children(g)` in
   stored order, seeded with a clone of the first child's value, then
   `op(acc, &v_i)` for children 2..k. A gate with k children costs one
   clone and k - 1 calls.
4. Stop at the first error in this fixed order. A child without a value
   would be an invariant break (impossible after a topological order); it
   is returned as `ModelError::UnknownNode` rather than a panic (Q3).

Built-in domains:

- `MaxMin<T: Ord + Clone>` (zero-sized): OR = max, AND = SAND = min. On a
  tie the earlier child's value is kept, so the result is deterministic
  even for types whose `Ord` ignores part of the value.
- `MinTime` (`u64`): OR = min, AND = max (parallel), SAND = saturating sum
  (sequential). Saturates at `u64::MAX` instead of panicking.

### `Evaluation<V>` layout

Derived data (D10), separate from the tree: `values: Vec<Option<V>>` in
arena order, the root index, and the root's value moved out of its slot into
a separate field. Moving it out (an `Option::take`, no clone) lets
`root_value()` return `&V` without a panic path. `get(idx)` is O(1) and
returns `None` for an index outside the tree; `iter()` yields
`(NodeIdx, &V)` in arena order. `Evaluation` derives `Clone`, `Debug`,
`PartialEq`, `Eq` when `V` does.

Considered: converting to a checked `Vec<V>` at the end. That costs a second
O(V) pass and a second allocation (the layouts of `Option<V>` and `V`
differ for most `V`), to save the tag: 1 byte per node for `u8`
(`Option<u8>` is 2 bytes), 8 bytes per node for `u64` (alignment), nothing
for niche types such as `Box` or `NonZeroU8`. At `large` with `u64` that is
0.8 MB, about 0.3% of the 300 MB client budget. Not worth it now (P3).
Explain-why traces and the proptest suite can add fields without changing
this layout.

## Complexity

`evaluate`: time O(V + E), plus V calls of `leaf` (steps only) and E - G
domain calls (G = gates, since each gate's first child is cloned, not
combined). Extra space O(V): the order (4 B/node) and the values. No
recursion, so a 100,000-deep chain runs on the default test thread
(`deep_chain_eval`). Nothing is O(V^2) (A9).

The tests prove the counts: on a diamond, each step's leaf call runs once
and the domain runs sum(children - 1) times (`evaluates_each_node_once`); on
a 64-level ladder with 2^64 root-to-leaf paths, leaf calls equal the number
of steps and domain calls equal E - G (`ladder_is_linear`).

## Limitation: shared subtrees in non-idempotent domains

Each node is evaluated once and its value reused by every parent (A2). For
idempotent domains (max, min, any, all) that is exact. For sum-like domains
(minimum cost, SAND time) and probability, a step shared by two branches of
an AND contributes once per path that reaches it: cost is counted twice,
and independent-probability products treat the two uses as independent
events. Example: AND over two gates that both need the same "obtain key fob"
step of cost 10 gives 20, not 10.

The exact answer needs a decision diagram (BDD) or a set-based domain,
which is out of scope here (A4, `docs/brainstorm.md` section 8.3). Until
then, `MinTime`'s SAND sum over shared steps is an upper bound, and the doc
comment of `evaluate` states the limitation.

## Numbers

Machine: Apple M2 Max (12 cores, 32 GB), macOS 26.6.2, `rustc 1.99.1
(58cf8f9c3 2026-10-13) (Homebrew)`, `cargo bench --bench eval` (release
profile), base commit `2c180d6` plus this job. divan 0.1.21, 20 samples, one
iteration per sample.

Workload (`benches/eval.rs`): random tree built through the public API,
breadth-first, 1 to 4 children per gate, about 40% gates in mixed kinds,
then link attempts for 3% of the node count (half to steps, half to deeper
gates); 1.9% become real links. `MaxMin<u8>` leaf values from a slice.
Building is outside the timed region.

| size | nodes | edges | links | `evaluate` median | ns/node | budget | share of budget |
|---|---:|---:|---:|---:|---:|---:|---:|
| large | 100,000 | 101,904 | 1,905 | 1.948 ms | 19.5 | 200 ms | 1.0% |
| stress | 1,000,000 | 1,018,611 | 18,612 | 20.56 ms | 20.6 | 3 s | 0.7% |

Context (`topological_order_only`): 1.29 ms at `large`, 13.17 ms at
`stress`, so about two thirds of `evaluate` is Kahn's order (in-degree
array, queue, output `Vec`), and the fold plus the result `Vec` is the other
third.

### What the numbers teach

1. **Linear in practice, not only on paper.** ns/node stays at about 20
   from 100,000 to 1,000,000 nodes: no hidden super-linear term, and the
   bigger arrays still stream well through the cache.
2. **The budget is far away.** Full recompute uses about 1% of its budget,
   so incremental recompute (A1, next milestones) is needed for the 5 ms
   "one rating change" budget, not for this one. Optimizing the full pass
   (reusing the order and value buffers, R2) would gain at most the 1.3 ms
   of the order; not worth the complexity now (P3).
3. **The order dominates.** If profiling of the full pass ever matters,
   caching the topological order between recomputes (it only changes on
   structural edits) is the first lever, not the fold.

## Consequences

- New public API: `AttributeDomain`, `evaluate`, `Evaluation`, `MaxMin`,
  `MinTime`; new errors `ModelError::EmptyGate` and
  `ModelError::MissingLeafValue` (both carry a `NodeId`, never a label).
- `src/eval.rs` touches tree internals only through the public API, so it
  can move to a future `aegis-engine` crate unchanged.
- Next backlog items build on this: explain-why traces (add fields to
  `Evaluation`) and a random-DAG proptest suite against a naive evaluator
  (`tests/eval.rs` already has a naive evaluator on fixed fixtures).
- CI bench gating (the 10% regression rule) is still out of scope.

## Reproduce

From `crates/aegis-model/`:

```
cargo bench --bench eval    # evaluate_max_min and topological_order_only at both sizes
cargo test --test eval      # values, order, exactly-once, ladder, errors, naive agreement
cargo test --benches        # every bench once (smoke test)
```
