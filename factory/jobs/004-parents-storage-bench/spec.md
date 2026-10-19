# 004-parents-storage-bench: lesson benchmark for linked-parent storage in aegis-model

## Open questions

These have defaults below. The approver can accept them or escalate.

1. **What share pattern counts as "realistic"?** No reference-project generator exists yet, and no document says how many nodes are shared or by how many gates. Default: three share rates, 1%, 10%, and 50% of non-root nodes shared, each shared node with 1 to 3 extra linked parents (uniform, fixed seed). The 50% row is there to show where the sparse map stops winning.
2. **Does this job switch `AttackTree` to the winner?** Default: **no**. This job only measures and records. `AttackTree` keeps `Vec<Vec<NodeIdx>>`. If the ADR recommends a different layout, the change is a follow-up job named in the ADR's Consequences (P3: complexity must earn its place first).
3. **criterion or divan?** Default: **divan**. It has fewer dependencies (R10), and its generic benches (`types = [...]`) run one bench body over all three stores, which suits a lesson. criterion's saved baselines (the CI 10% regression gate, section 8) are not needed for this comparison. CI bench gating is out of scope.

## Goal

An engineer learning the codebase can run two commands and see, at the `large` (100,000 nodes) and `stress` (1,000,000 nodes) reference sizes, how three layouts for linked parents compare: per-node `Vec<NodeIdx>` (today), per-node `Box<[NodeIdx]>`, and a sparse `BTreeMap<NodeIdx, Vec<NodeIdx>>` holding shared nodes only. They see heap bytes from `dhat`, and `link` and `parents()` time from `divan`. The numbers, what they teach, and the decision about `AttackTree`'s layout are recorded in `docs/adr/0004-linked-parents-storage.md` (P2, L1, L2). No user-visible product change.

## Scope

In:
- Three bench-local store types behind one small trait, a deterministic generator of link lists, and two bench targets: `parents_time` (divan) and `parents_memory` (dhat, `harness = false`).
- An integration test proving the three stores agree with each other and with `AttackTree::parents` on the same links.
- Dev-dependencies `divan` and `dhat` (both MIT OR Apache-2.0), with `Cargo.lock` updated.
- `docs/adr/0004-linked-parents-storage.md` with the measured tables and the decision. One line added to ADR 0002's Consequences pointing to ADR 0004.

Out:
- Changing `AttackTree`'s storage or any file under `src/` (see open question 2).
- Other layouts (`SmallVec`, `FxHashMap`, CSR or offset arrays). The ADR may list them as ideas, without numbers.
- A general reference-project generator crate, a Cargo workspace, and CI bench gating.
- Cycle checks and duplicate checks in the stores. Those live in `AttackTree::link` and do not depend on the layout.

## Design notes

Crate: `crates/aegis-model/` only. Files:
- `Cargo.toml`: `[dev-dependencies] divan`, `dhat`; `[[bench]] name = "parents_time", harness = false` and `[[bench]] name = "parents_memory", harness = false`. Keep `rust-version = "1.85"`. The 2024-edition resolver picks MSRV-compatible versions. If no compatible version exists, stop and report; do not raise the MSRV.
- `benches/common/mod.rs`: stores, trait, generator. Shared with `mod common;` in each bench, and with `#[path = "../benches/common/mod.rs"] mod common;` in the test. Each file starts with `#![forbid(unsafe_code)]`. Item-level `#[allow(dead_code)]` is allowed only where one target does not use an item, with a comment saying why.
- `benches/parents_time.rs`, `benches/parents_memory.rs`, `tests/parents_storage.rs`.
- `docs/adr/0004-linked-parents-storage.md`, and one line in `docs/adr/0002-dag-links-and-cycle-detection.md`.

`NodeIdx` has no public constructor. Inside the benches, use a local `#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)] struct Ix(u32)` with the same 4-byte layout. The test maps `Ix` to the real `NodeIdx` values returned by `add_child`, and the arena index is the same.

**Trait** (only linked parents. The owner lives in a separate `owners` array that is identical for all layouts, so it is left out):
```rust
trait ParentStore {
    const NAME: &'static str;
    fn with_nodes(n: usize) -> Self;          // empty store for n nodes
    fn link(&mut self, child: Ix, parent: Ix); // append, keep link order
    fn linked(&self, child: Ix) -> impl Iterator<Item = Ix> + '_; // link order
}
```

| Store | `link` | `linked` | Live heap |
|---|---|---|---|
| `VecStore(Vec<Vec<Ix>>)` (today's layout) | O(1) amortized push | O(1) + O(k) | 24 B x V + 4 B x sum of capacities, one block per shared node |
| `BoxStore(Vec<Box<[Ix]>>)` | O(k): `into_vec`, push, `into_boxed_slice` (a realloc on every link, with no spare capacity) | O(1) + O(k) | 16 B x V + 4 B x sum of k, exact |
| `SparseStore(BTreeMap<Ix, Vec<Ix>>)` | O(log S) | O(log S) + O(k), also for a miss | O(S) map nodes + Vec payloads; unshared nodes cost 0 |

V = nodes, S = shared nodes, k = linked parents of one node. Every store is built with exact outer capacity (`with_capacity(V)`), so the comparison measures layout, not growth slack. All three iterate in link order, so they are deterministic (P4, D7).

**Generator** `fn shape(n: usize, share_pct: u32, seed: u64) -> Shape`, with `Shape { owners: Vec<Ix>, links: Vec<(Ix /*child*/, Ix /*parent*/)> }`:
- Node 0 is the root. Node i >= 1 has owner `j < i`. A shared child gets 1 to 3 distinct linked parents `p < i`, `p != owner`. So every edge goes from a lower to a higher index, which makes the graph acyclic and valid for `AttackTree::link`.
- `links` is shuffled (Fisher-Yates) so links to the same child are interleaved, as in real editing. This matters for `BoxStore` reallocs and `BTreeMap` insert order.
- Random numbers come from an inline SplitMix64 with a fixed seed (no `rand` dependency). The same arguments give the same `Shape`.
- Time O(V + L), L = links. Cases: sizes {100_000, 1_000_000} x share {1, 10, 50}. Shapes are built once per case (`OnceLock` or built outside the timed and measured region).

**`parents_time` (divan)**, generic over the three stores (`types = [...]`), `args` over the six cases:
- `link`: build an empty store and apply all L links. Counter: `ItemsCount(L)`, so divan reports time per link.
- `parents_all`: on a prebuilt store, sum the `linked(i)` of every node i in 0..V into `black_box`. This is the recompute access pattern, where most lookups miss. Counter `ItemsCount(V)`.
- Bound the run time with `sample_count` and `sample_size` (or `max_time`) so `cargo bench --bench parents_time` finishes in under 10 minutes on the dev machine.
- One extra context bench: `AttackTree::link` for 1,000 links on the `large` 10% shape, to show how much of a real `link` call is storage and how much is the cycle DFS plus the O(V) visited allocation.

**`parents_memory` (dhat, `harness = false`)**: `#[global_allocator] static ALLOC: dhat::Alloc = dhat::Alloc;`, one profiler in testing mode (it must not write `dhat-heap.json` into the repo). For each case and store: build the `Shape` first, read `HeapStats::get()`, build the store, read again. Report:
- live bytes (`curr_bytes` delta), bytes per node, and live blocks;
- total allocations during the build (`total_blocks` delta), which shows realloc churn.

It prints one Markdown table to stdout, sorted by size, share, then store. It then checks two facts that hold for these layouts, and exits non-zero if either fails:
- (a) `BoxStore` live bytes <= `VecStore` live bytes in every case;
- (b) `SparseStore` live bytes < `VecStore` live bytes at 1% share.

When run without `--bench` (as `cargo test --benches` does), both benches still run every case once, as a smoke test.

**ADR 0004** (L1, L2): context (ADR 0002 picked `Vec<Vec<NodeIdx>>`, with 24 B per node even when unused), the three options with big-O, both tables copied from real runs (with machine, OS, `rustc --version`, and commit), what the numbers teach (header cost vs sparsity, realloc churn of exact-size boxes, the log S lookup on misses, the share rate where sparse stops winning), the context bench's share of a `link` call, bytes per node against the `large` memory budget (300 MB client), the decision for `AttackTree` (keep, or switch in a named follow-up job), and the command lines to reproduce.

## Acceptance criteria

All commands run from `crates/aegis-model/` unless they start with `../../`.

- [ ] AC1 The three stores agree: for shapes with n in {1, 2, 50, 2_000} and share in {0, 1, 10, 50, 100}, and for a proptest over random (n <= 500, share, seed) with at least 64 cases, `linked(i)` collects to the same `Vec` for all three stores and every i, in link order. Test: `cargo test --test parents_storage stores_agree`.
- [ ] AC2 The stores match the real model: build an `AttackTree` (all gates) from `shape(2_000, 10, seed).owners`, apply the same `links` with `AttackTree::link` (every call returns `Ok`), then for every node `tree.parents(idx)` equals `[owner] ++ VecStore.linked(i)`. Test: `cargo test --test parents_storage matches_attack_tree`.
- [ ] AC3 The generator is deterministic and valid: same arguments give equal shapes; different seeds give different `links`; every link has `parent < child` and `parent != owner(child)`; no duplicate (child, parent) pair; each shared child has 1 to 3 linked parents; the shared count is within +/-1 node of `share_pct`% of `n - 1`. Test: `cargo test --test parents_storage shape_`.
- [ ] AC4 `cargo test --benches` passes: both bench binaries run every case once, and `parents_memory` exits 0 (facts (a) and (b) hold).
- [ ] AC5 `cargo bench --bench parents_time` prints `link` and `parents_all` for all 3 stores x 2 sizes x 3 share rates (36 rows) plus the `AttackTree::link` context bench, in under 10 minutes. Check: `cargo bench --bench parents_time 2>&1 | tee /tmp/t.txt; grep -cE 'VecStore|BoxStore|SparseStore' /tmp/t.txt` counts at least 36 data rows (or as many as divan's tree output needs).
- [ ] AC6 `cargo bench --bench parents_memory` prints a Markdown table with 18 data rows (3 stores x 2 sizes x 3 shares) and the columns size, share %, store, live bytes, bytes/node, live blocks, build allocations, and exits 0.
- [ ] AC7 No dhat output file is left behind. Check: after AC4 to AC6, `git -C ../.. status --porcelain | grep -c dhat` prints 0.
- [ ] AC8 Only allowed licenses: `../../scripts/deny.sh licenses bans sources` passes with the new dev-dependencies. `Cargo.toml` still has `rust-version = "1.85"`, and `divan` and `dhat` are pinned to a major or minor version (no `*`).
- [ ] AC9 ADR exists and is complete: `../../docs/adr/0004-linked-parents-storage.md` has the sections Context, Options, Numbers, What the numbers teach, Decision, Consequences, and Reproduce. Its Numbers section holds both tables with all 18 memory rows and the 36 timing rows (or a per-case summary of them), plus machine and `rustc` version. Check: `grep -nE '^## (Context|Options|Numbers|What the numbers teach|Decision|Consequences|Reproduce)' ../../docs/adr/0004-linked-parents-storage.md` prints 7 lines.
- [ ] AC10 ADR 0002 points to ADR 0004. Check: `grep -n '0004' ../../docs/adr/0002-dag-links-and-cycle-detection.md` finds one line.
- [ ] AC11 The library is unchanged, and nothing outside scope is touched. Check: `git -C ../.. diff --stat master -- crates/aegis-model/src spikes docs/decisions.md docs/engineering-standards.md` is empty.
- [ ] AC12 Each bench and the test file has `#![forbid(unsafe_code)]`, and every public-ish item in `benches/common/mod.rs` (trait, stores, `shape`) has a doc comment that states its time and space cost (A8) and what it teaches (L2). Check: `grep -L 'forbid(unsafe_code)' benches/*.rs benches/common/mod.rs tests/parents_storage.rs` prints nothing. Doc comments are checked by review.

## Checks

From `/Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model`:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --benches
cargo bench --bench parents_memory
cargo bench --bench parents_time
../../scripts/deny.sh licenses bans sources
```
