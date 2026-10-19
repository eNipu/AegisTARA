# ADR 0004: Storage layout for linked parents

- Status: proposed (job 004-parents-storage-bench)
- Date: 2026-10-19
- Crate: `crates/aegis-model` (benches and tests only; `src/` unchanged)
- Rules: P2, P3, P4, D6, D7, A8, L1, L2 in `docs/engineering-standards.md`
- Extends: `0002-dag-links-and-cycle-detection.md`

## Context

ADR 0002 stores the extra parents that `AttackTree::link` adds as
`linked_parents: Vec<Vec<NodeIdx>>`, one `Vec` per node. An empty `Vec` does
not allocate, but its 24-byte header is paid by every node, shared or not.
In real attack trees most nodes are not shared, so most of those headers
hold nothing. ADR 0002 picked the layout without numbers. This record
measures it against two alternatives at the `large` (100,000 nodes) and
`stress` (1,000,000 nodes) reference sizes, so the choice rests on evidence
(P2) and the benches stay as a lesson (L2).

This job only measures. `AttackTree` is not changed here (spec open
question 2): if a different layout had won, the switch would be a named
follow-up job.

## Options

Notation: V = nodes, S = shared nodes (at least one linked parent), k =
linked parents of one node, L = links.

| Store | `link` | `linked` (lookup) | Live heap |
|---|---|---|---|
| `VecStore(Vec<Vec<Ix>>)`, today | O(1) amortized push | O(1) + O(k) | 24 B x V + 4 B x capacity per shared node, one block per shared node |
| `BoxStore(Vec<Box<[Ix]>>)` | O(k): `into_vec`, push, `into_boxed_slice` (reallocates every link) | O(1) + O(k) | 16 B x V + exactly 4 B x k per shared node |
| `SparseStore(BTreeMap<Ix, Vec<Ix>>)` | O(log S) | O(log S) + O(k), a miss also pays O(log S) | O(S) B-tree nodes + one `Vec` per shared node; unshared nodes cost 0 |

All three keep link order, so `parents()` stays deterministic (P4). The
`BTreeMap` (not a hash map) keeps any future iteration in key order (D7).
The benches use a bench-local `Ix(u32)` with the same 4-byte layout as
`NodeIdx`. `tests/parents_storage.rs` proves the three stores give identical
answers, and that `VecStore` matches `AttackTree::parents` on the same links.

Not measured (ideas only, no numbers): `SmallVec<[NodeIdx; 1]>` or `[..; 2]`
inline (D6), `FxHashMap` instead of `BTreeMap`, and a CSR or offset-array
layout rebuilt from an edge list (cheap reads, expensive edits).

### Workload

`benches/common/mod.rs::shape(n, share_pct, seed)`: node 0 is the root, node
`i` gets a uniform owner `j < i` (random recursive tree). `share_pct`% of the
non-root nodes are shared, each with 1 to 3 (uniform) distinct linked parents
`p < i`, `p != owner`. Links are shuffled (Fisher-Yates) so links to one child
interleave, as in real editing. SplitMix64 with a fixed seed: the same
arguments give the same graph. Share rates 1%, 10%, and 50% (spec open
question 1); 50% is there to find where the sparse map stops winning.

## Numbers

Machine: Apple M2 Max (12 cores, 32 GB), macOS 26.6.2, `rustc 1.99.1
(58cf8f9c3 2026-10-13) (Homebrew)`, `cargo bench` (release profile), base
commit `0bce7bb` plus this job's benches. divan 0.1.21, dhat 0.3.3.

### Memory (`cargo bench --bench parents_memory`, dhat)

Live bytes and blocks are the store alone (the shape is built before the
first reading). Build allocations count every allocation and reallocation
during the build (dhat `total_blocks`).

| size | nodes | share % | links | store | live bytes | bytes/node | live blocks | build allocations |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| large | 100000 | 1 | 2009 | VecStore | 2416000 | 24.16 | 1001 | 1001 |
| large | 100000 | 1 | 2009 | BoxStore | 1608036 | 16.08 | 1001 | 4019 |
| large | 100000 | 1 | 2009 | SparseStore | 59776 | 0.59 | 1132 | 1132 |
| large | 100000 | 10 | 20098 | VecStore | 2560000 | 25.60 | 10001 | 10001 |
| large | 100000 | 10 | 20098 | BoxStore | 1680392 | 16.80 | 10001 | 40197 |
| large | 100000 | 10 | 20098 | SparseStore | 602272 | 6.02 | 11335 | 11335 |
| large | 100000 | 50 | 99970 | VecStore | 3200000 | 32.00 | 50001 | 50001 |
| large | 100000 | 50 | 99970 | BoxStore | 1999880 | 19.99 | 50001 | 199941 |
| large | 100000 | 50 | 99970 | SparseStore | 3012896 | 30.12 | 56678 | 56678 |
| stress | 1000000 | 1 | 19784 | VecStore | 24160000 | 24.16 | 10001 | 10001 |
| stress | 1000000 | 1 | 19784 | BoxStore | 16079136 | 16.07 | 10001 | 39569 |
| stress | 1000000 | 1 | 19784 | SparseStore | 597504 | 0.59 | 11321 | 11321 |
| stress | 1000000 | 10 | 199641 | VecStore | 25600000 | 25.60 | 100001 | 100001 |
| stress | 1000000 | 10 | 199641 | BoxStore | 16798564 | 16.79 | 100001 | 399283 |
| stress | 1000000 | 10 | 199641 | SparseStore | 6024608 | 6.02 | 113358 | 113358 |
| stress | 1000000 | 50 | 999452 | VecStore | 32000000 | 32.00 | 500001 | 500001 |
| stress | 1000000 | 50 | 999452 | BoxStore | 19997808 | 19.99 | 500001 | 1998905 |
| stress | 1000000 | 50 | 999452 | SparseStore | 30094944 | 30.09 | 566701 | 566701 |

The bench then checks (a) `BoxStore` <= `VecStore` in every case and (b)
`SparseStore` < `VecStore` at 1% share; both hold.

### Time (`cargo bench --bench parents_time`, divan)

Median of up to 100 samples, one iteration per sample (`sample_size = 1`,
`max_time = 1` s per bench). `link` = build an empty store for V nodes and
apply all L links (so dense stores include their O(V) header setup); per-item
time is median / L. `parents_all` = iterate `linked(i)` for every node i in
0..V on a prebuilt store; per-item time is median / V. All 36 rows:

| size | share % | store | `link` median | ns/link | `parents_all` median | ns/node |
|---|---:|---|---:|---:|---:|---:|
| large | 1 | VecStore | 47.49 µs | 23.6 | 62.16 µs | 0.62 |
| large | 1 | BoxStore | 85.91 µs | 42.8 | 62.37 µs | 0.62 |
| large | 1 | SparseStore | 85.29 µs | 42.5 | 974.2 µs | 9.74 |
| large | 10 | VecStore | 192.7 µs | 9.6 | 148.7 µs | 1.49 |
| large | 10 | BoxStore | 752.0 µs | 37.4 | 143.6 µs | 1.44 |
| large | 10 | SparseStore | 1.199 ms | 59.7 | 1.680 ms | 16.8 |
| large | 50 | VecStore | 843.1 µs | 8.4 | 584.1 µs | 5.84 |
| large | 50 | BoxStore | 3.683 ms | 36.8 | 553.1 µs | 5.53 |
| large | 50 | SparseStore | 7.241 ms | 72.4 | 3.104 ms | 31.0 |
| stress | 1 | VecStore | 542.8 µs | 27.4 | 643.6 µs | 0.64 |
| stress | 1 | BoxStore | 885.1 µs | 44.7 | 646.1 µs | 0.65 |
| stress | 1 | SparseStore | 1.337 ms | 67.6 | 12.58 ms | 12.6 |
| stress | 10 | VecStore | 2.167 ms | 10.9 | 1.546 ms | 1.55 |
| stress | 10 | BoxStore | 7.742 ms | 38.8 | 1.504 ms | 1.50 |
| stress | 10 | SparseStore | 15.83 ms | 79.3 | 19.44 ms | 19.4 |
| stress | 50 | VecStore | 13.03 ms | 13.0 | 6.009 ms | 6.01 |
| stress | 50 | BoxStore | 51.30 ms | 51.3 | 5.650 ms | 5.65 |
| stress | 50 | SparseStore | 102.8 ms | 102.9 | 35.43 ms | 35.4 |

Context bench `attack_tree_link_1000`: 1,000 real `AttackTree::link` calls
on the `large` 10% shape (fresh tree clone per sample): median 2.139 ms,
that is about 2.14 µs per call.

## What the numbers teach

1. **Header cost versus sparsity.** The dense layouts pay per node, the
   sparse map per shared node. `VecStore` costs 24 B/node plus 16 B per
   shared node (a 4-slot buffer: 1 to 3 links never trigger a regrowth).
   `SparseStore` costs about 60 B per shared node (0.59 B/node at 1%, 6.02 at
   10%, 30.1 at 50%: the key, the 24 B `Vec` header, B-tree node slack, and
   the 16 B buffer). The lines cross where 24 + 16s = 60s, at about
   **s = 55% share**: at 50% the map is already within 6% of `VecStore`.
   Against `BoxStore` (16 B + about 8 B per shared node, exact) the map
   stops winning at about 30% share.
2. **Exact-size boxes buy memory with churn.** `BoxStore` is always smallest
   among the dense layouts (8 B per node less than `VecStore`), but it makes
   about 2 allocator calls per link (grow, then shrink to exact size) versus
   1 per shared node for `VecStore`: 4x the build allocations at every share
   rate, and 1.6 to 4.4x the `link` time.
3. **A sparse lookup pays log S on every miss.** `parents_all` is the
   full-recompute access pattern, where most nodes are not shared. Dense
   layouts answer in 0.6 to 6 ns per node; `SparseStore` needs 10 to 35 ns
   per node, 5 to 20x slower, because every miss still searches the tree and
   the memory accesses are scattered. Dense `Vec` and `Box` read at the same
   speed: their difference is only in the header size.
4. **Storage is a rounding error in a real `link`.** A real `AttackTree::link`
   costs about 2.14 µs, dominated by the cycle DFS and the zeroed O(V)
   visited allocation (100,000 bytes). The `VecStore` push is about 10 ns of
   that (under 0.5%), and even `SparseStore` (60 ns) is under 3%. Layout does
   not matter for edit latency; ADR 0002's batch-validation upgrade path does.
5. **Against the budget.** At `large`, `VecStore` holds 2.4 to 3.2 MB, about
   1% of the 300 MB client memory budget. The best alternative would save
   at most about 2.4 MB (0.8% of the budget) at the price of a 5 to 16x
   slower full-recompute read path (`SparseStore`) or 4x allocation churn
   (`BoxStore`). The full-recompute budget (200 ms at `large`) is not at risk
   for any layout: `parents_all` is at most 3.1 ms.

## Decision

**Keep `Vec<Vec<NodeIdx>>` in `AttackTree`.** It has the fastest `link`, the
fastest (tied) lookups, and one allocation per shared node. Its extra memory
(about 8 B per node over `BoxStore`, about 24 B per node over `SparseStore`
at low share rates) is about 1% of the `large` budget. By P3, a more complex
layout must earn its place with numbers, and these numbers do not show a
budget it would fix. No follow-up job is created.

## Consequences

- `AttackTree` stays as in ADR 0002; no code in `src/` changed.
- The benches stay in `crates/aegis-model/benches/` as a lesson (L2), and
  `cargo test --benches` runs them once as a smoke test. `parents_memory`
  fails if `BoxStore` ever uses more than `VecStore`, or `SparseStore` stops
  winning at 1% share.
- Revisit if a profile of the real client shows the 24 B/node header
  matters (for example, many projects open at once), or if real projects
  turn out to share far less than 1% of nodes. `SparseStore` is then the
  memory choice and `BoxStore` the middle ground; `SmallVec` inline storage
  (D6) is worth adding to these benches before deciding.
- New dev-dependencies: `divan` 0.1 and `dhat` 0.3 (both MIT OR Apache-2.0,
  MSRV 1.85 kept). They are not in the library's dependency tree.
- CI bench gating (the 10% regression rule) is still out of scope.

## Reproduce

From `crates/aegis-model/`:

```
cargo bench --bench parents_memory   # Markdown memory table, exits non-zero if (a) or (b) fails
cargo bench --bench parents_time     # divan tree: link and parents_all, plus attack_tree_link_1000
cargo test --test parents_storage    # the stores agree with each other and with AttackTree
cargo test --benches                 # both benches, every case once (smoke test)
```

`parents_memory` runs dhat in testing mode, so it writes no `dhat-heap.json`.
On the machine above, `parents_memory` takes about 20 s and `parents_time`
about 14 s.
