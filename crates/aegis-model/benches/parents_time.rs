//! Time of `link` and `parents()` for three linked-parent layouts, with divan.
//!
//! Run: `cargo bench --bench parents_time` (under `cargo test --benches`,
//! divan runs every bench once as a smoke test).
//!
//! - `link`: build an empty store and apply all L links of a case. Reported
//!   per link (`ItemsCount(L)`).
//! - `parents_all`: on a prebuilt store, sum `linked(i)` for every node i in
//!   0..V. This is the full-recompute access pattern, where most lookups
//!   miss. Reported per node (`ItemsCount(V)`).
//! - `attack_tree_link_1000`: context. 1,000 real `AttackTree::link` calls
//!   on the `large` 10% shape, to compare with the storage-only `link` cost:
//!   the rest is the duplicate scan, the cycle DFS, and the O(V) visited
//!   allocation.
//!
//! What it teaches (ADR 0004): dense layouts index in O(1), the sparse map
//! pays O(log S) on every lookup (also on a miss), and exact-size boxes pay a
//! reallocation on every link.

#![forbid(unsafe_code)]

mod common;

use std::sync::OnceLock;

use aegis_model::{AttackTree, GateKind, Node, NodeId, NodeIdx};
use common::{BoxStore, CASES, Case, Ix, ParentStore, SEED, Shape, SparseStore, VecStore, build};
use divan::counter::ItemsCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// The shape of `case`, built once per process and shared by every bench
/// and store, outside the timed region. Time O(V + L) on first use.
fn shape_of(case: Case) -> &'static Shape {
    static SHAPES: [OnceLock<Shape>; CASES.len()] = [const { OnceLock::new() }; CASES.len()];
    let slot = CASES.iter().position(|c| *c == case).unwrap_or(0);
    SHAPES[slot].get_or_init(|| common::shape(case.nodes, case.share_pct, SEED))
}

/// Builds an empty store and applies every link of the case.
#[divan::bench(
    types = [VecStore, BoxStore, SparseStore],
    args = CASES,
    sample_size = 1,
    max_time = 1,
)]
fn link<S: ParentStore>(bencher: Bencher, case: Case) {
    let shape = shape_of(case);
    bencher
        .counter(ItemsCount::new(shape.links.len()))
        .bench_local(|| build::<S>(shape.owners.len(), black_box(&shape.links)));
}

/// Sums the linked parents of every node on a prebuilt store.
#[divan::bench(
    types = [VecStore, BoxStore, SparseStore],
    args = CASES,
    sample_size = 1,
    max_time = 1,
)]
fn parents_all<S: ParentStore>(bencher: Bencher, case: Case) {
    let shape = shape_of(case);
    let n = shape.owners.len();
    let store: S = build(n, &shape.links);
    bencher.counter(ItemsCount::new(n)).bench_local(|| {
        let store = black_box(&store);
        let mut sum = 0_u64;
        for i in 0..n {
            for p in store.linked(Ix::from_usize(i)) {
                sum += u64::from(p.0);
            }
        }
        sum
    });
}

/// Number of real `AttackTree::link` calls in the context bench.
const CONTEXT_LINKS: usize = 1_000;

/// A tree built from the `large` 10% owners (all gates), the `NodeIdx` of
/// every node, and the shape it came from. Built once, outside timing.
struct Context {
    tree: AttackTree,
    idx: Vec<NodeIdx>,
    shape: &'static Shape,
}

/// Builds the context tree. Time O(V). Returns `None` if the model rejects
/// a node, which `shape`'s contract rules out.
fn context() -> Option<&'static Context> {
    static CONTEXT: OnceLock<Option<Context>> = OnceLock::new();
    CONTEXT
        .get_or_init(|| {
            let case = CASES.get(1).copied()?;
            let shape = shape_of(case);
            let mut tree =
                AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "n0")).ok()?;
            let mut idx = Vec::with_capacity(shape.owners.len());
            idx.push(tree.root());
            for (i, owner) in shape.owners.iter().enumerate().skip(1) {
                let parent = *idx.get(owner.index())?;
                let label = format!("n{i}");
                let node = Node::gate(GateKind::Or, label);
                idx.push(tree.add_child(parent, NodeId::new_v7(), node).ok()?);
            }
            Some(Context { tree, idx, shape })
        })
        .as_ref()
}

/// Context: the first 1,000 links of the `large` 10% shape through the real
/// `AttackTree::link` (duplicate scan + cycle DFS + O(V) visited flags +
/// storage), on a fresh clone of the tree each sample.
#[divan::bench(sample_size = 1, max_time = 2)]
fn attack_tree_link_1000(bencher: Bencher) {
    let Some(ctx) = context() else {
        eprintln!("context tree could not be built");
        return;
    };
    let links: Vec<(NodeIdx, NodeIdx)> = ctx
        .shape
        .links
        .iter()
        .take(CONTEXT_LINKS)
        .filter_map(|(c, p)| Some((*ctx.idx.get(p.index())?, *ctx.idx.get(c.index())?)))
        .collect();
    bencher
        .counter(ItemsCount::new(links.len()))
        .with_inputs(|| ctx.tree.clone())
        .bench_local_values(|mut tree| {
            for &(parent, child) in &links {
                let _ = black_box(tree.link(parent, child));
            }
            tree
        });
}
