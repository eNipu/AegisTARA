//! Full evaluation time against the "full recompute" budget (engineering
//! standards section 2): at most 200 ms at `large` (100,000 nodes) and 3 s
//! at `stress` (1,000,000 nodes). Results are recorded in ADR 0005.
//!
//! Run: `cargo bench --bench eval` (under `cargo test --benches`, divan runs
//! every bench once as a smoke test).
//!
//! Workload: a random tree built through the public API, breadth-first, each
//! gate with 1 to 4 children, about 40% of new nodes gates (And, Or, SeqAnd
//! in equal parts), then `SHARE_PCT`% of the node count `link` attempts,
//! half to steps, half to gates deeper down. Attempts that would point to a
//! smaller index or repeat an edge are skipped, so about 1.9% of the node
//! count become real links (the bench prints the count). Leaf values are `MaxMin<u8>`
//! levels read from a slice. Building the tree is outside the timed region;
//! only `evaluate` is timed (topological order + fold + result). For
//! context, `topological_order_only` times the first phase alone.

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::sync::OnceLock;

use aegis_model::{AttackTree, GateKind, MaxMin, ModelError, Node, NodeId, NodeIdx, evaluate};
use divan::counter::ItemsCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// Reference sizes from the engineering standards.
const SIZES: [usize; 2] = [100_000, 1_000_000];
/// `link` attempts, in percent of the node count.
const SHARE_PCT: usize = 3;
/// Fixed seed, so every run measures the same tree.
const SEED: u64 = 0x5EED_0005;

/// Deterministic SplitMix64.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish in `0..n` (`n > 0`); modulo bias is irrelevant here.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap_or(0)
    }
}

/// A tree and the leaf level of every node (by arena position; 0 for gates).
struct Fixture {
    tree: AttackTree,
    levels: Vec<u8>,
}

/// Builds a tree of exactly `n` nodes, breadth-first, in which every gate
/// has at least one child, then tries `n * SHARE_PCT / 100` links. Every
/// link goes from a gate to a node with a larger index, and every
/// `add_child` edge does too, so no link can close a cycle. Time O(n) plus
/// the cost of the links (each one a DFS over the descendants of its child,
/// which are small because children are picked from the deeper half).
fn build(n: usize, seed: u64) -> Result<Fixture, ModelError> {
    const KINDS: [GateKind; 3] = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
    let mut rng = Rng(seed);
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let mut gates = vec![tree.root()];
    let mut steps = Vec::new();
    // Gates without children yet, in creation order (breadth-first). Each
    // one has a reserved slot, so none stays empty: `tree.len() +
    // queue.len() <= n` holds throughout, so the loop ends with an empty
    // queue. When the queue runs dry early, random gates get more children.
    let mut queue = VecDeque::from([tree.root()]);
    while tree.len() < n {
        let g = match queue.pop_front() {
            Some(g) => g,
            None => gates[rng.below(gates.len())],
        };
        for _ in 0..=rng.below(4) {
            let room = n.saturating_sub(tree.len() + queue.len());
            if room == 0 {
                break;
            }
            if room > 1 && rng.below(10) < 4 {
                let kind = KINDS[rng.below(KINDS.len())];
                let c = tree.add_child(g, NodeId::new_v7(), Node::gate(kind, "g"))?;
                gates.push(c);
                queue.push_back(c);
            } else {
                steps.push(tree.add_child(g, NodeId::new_v7(), Node::step("s"))?);
            }
        }
    }
    let deep_gates = &gates[gates.len() / 2..];
    for k in 0..n * SHARE_PCT / 100 {
        let parent = gates[rng.below(gates.len())];
        let child = if k % 2 == 0 {
            steps[rng.below(steps.len())]
        } else {
            deep_gates[rng.below(deep_gates.len())]
        };
        if child > parent && !tree.children(parent).contains(&child) {
            tree.link(parent, child)?;
        }
    }
    let mut levels = vec![0_u8; tree.len()];
    for s in &steps {
        levels[s.index()] = u8::try_from(rng.below(5)).unwrap_or(0);
    }
    Ok(Fixture { tree, levels })
}

/// The fixture for `n` nodes, built once per process outside timing.
fn fixture(n: usize) -> Option<&'static Fixture> {
    static FIXTURES: [OnceLock<Option<Fixture>>; SIZES.len()] =
        [const { OnceLock::new() }; SIZES.len()];
    let slot = SIZES.iter().position(|s| *s == n)?;
    FIXTURES[slot]
        .get_or_init(|| match build(n, SEED) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("fixture for {n} nodes could not be built: {e}");
                None
            }
        })
        .as_ref()
}

/// Full evaluation of the whole tree with `MaxMin<u8>`.
#[divan::bench(args = SIZES, sample_count = 20, max_time = 10)]
fn evaluate_max_min(bencher: Bencher, n: usize) {
    let Some(f) = fixture(n) else {
        return;
    };
    let edges: usize = f
        .tree
        .iter()
        .map(|(i, _, _)| f.tree.children(i).len())
        .sum();
    eprintln!(
        "eval bench: {} nodes, {} edges, {} links",
        f.tree.len(),
        edges,
        edges + 1 - f.tree.len()
    );
    let domain = MaxMin::<u8>::new();
    let leaf = |i: NodeIdx| f.levels.get(i.index()).copied();
    bencher
        .counter(ItemsCount::new(f.tree.len()))
        .bench_local(|| evaluate(black_box(&f.tree), &domain, leaf));
}

/// Context: `AttackTree::topological_order` alone, the first phase of
/// `evaluate`. The difference to `evaluate_max_min` is the fold and the
/// result allocation.
#[divan::bench(args = SIZES, sample_count = 20, max_time = 10)]
fn topological_order_only(bencher: Bencher, n: usize) {
    let Some(f) = fixture(n) else {
        return;
    };
    bencher
        .counter(ItemsCount::new(f.tree.len()))
        .bench_local(|| black_box(&f.tree).topological_order());
}
