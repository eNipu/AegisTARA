//! Shared code for the linked-parents storage lesson (ADR 0004).
//!
//! Three layouts for the extra parents that `AttackTree::link` adds, behind
//! one small trait, plus a deterministic generator of link lists. Used by
//! `benches/parents_time.rs` (divan), `benches/parents_memory.rs` (dhat), and
//! `tests/parents_storage.rs` (agreement with each other and with
//! `AttackTree::parents`).
//!
//! The owner of a node lives in a separate `owners` array that is identical
//! for every layout, so the stores hold linked parents only.
//!
//! Notation: V = nodes, L = links, S = shared nodes (nodes with at least one
//! linked parent), k = linked parents of one node.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

/// Bench-local node index with the same 4-byte layout as
/// `aegis_model::NodeIdx` (which has no public constructor). The arena index
/// is the same, so `Ix(i)` stands for the node with `NodeIdx::index() == i`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ix(pub u32);

impl Ix {
    /// The arena position. Time O(1).
    pub fn index(self) -> usize {
        // u32 -> usize never truncates on the 32- and 64-bit targets we build.
        self.0 as usize
    }

    /// Builds an index from an arena position. Every caller passes a
    /// position below the node count of a `Shape`, which `shape` keeps below
    /// `u32::MAX`; out-of-range positions saturate. Time O(1).
    pub fn from_usize(i: usize) -> Self {
        Ix(u32::try_from(i).unwrap_or(u32::MAX))
    }
}

/// Storage for the linked parents of every node (not the owner).
///
/// The lesson: all three implementations give the same answers in the same
/// order (tested in `tests/parents_storage.rs`); they differ only in heap
/// bytes, allocation churn, and lookup cost, which the benches measure.
/// Cycle and duplicate checks are not here: they live in
/// `AttackTree::link` and do not depend on the layout.
pub trait ParentStore {
    /// Short name printed in the memory table.
    // `parents_time` labels rows with divan's type names instead, so the
    // constant is unused in that target only.
    #[allow(dead_code)]
    const NAME: &'static str;

    /// An empty store for `n` nodes, with exact outer capacity, so the
    /// comparison measures layout and not growth slack.
    fn with_nodes(n: usize) -> Self;

    /// Appends `parent` to the linked parents of `child`, keeping link order.
    /// `child` must be below the `n` given to `with_nodes`; out-of-range
    /// children are ignored (the benches never pass one).
    fn link(&mut self, child: Ix, parent: Ix);

    /// The linked parents of `child`, in link order. Empty for unshared or
    /// out-of-range nodes.
    fn linked(&self, child: Ix) -> impl Iterator<Item = Ix> + '_;
}

/// Today's layout in `AttackTree`: one `Vec` per node.
///
/// - `link`: O(1) amortized push. A `Vec` grows 0 -> 4 -> 8 elements (the
///   standard library's minimum non-zero capacity for 4-byte items is 4), so
///   1 to 3 links cost one allocation and leave spare capacity.
/// - `linked`: O(1) index + O(k) iteration.
/// - Live heap: 24 B x V for headers (paid even by unshared nodes) plus
///   4 B x capacity for each shared node, one block per shared node.
///
/// Teaches: the per-node header dominates when sharing is rare.
pub struct VecStore(Vec<Vec<Ix>>);

impl ParentStore for VecStore {
    const NAME: &'static str = "VecStore";

    /// Time O(V) (writes V empty headers), space 24 B x V.
    fn with_nodes(n: usize) -> Self {
        let mut v = Vec::with_capacity(n);
        v.resize_with(n, Vec::new);
        VecStore(v)
    }

    /// Time O(1) amortized, at most one (re)allocation.
    fn link(&mut self, child: Ix, parent: Ix) {
        if let Some(list) = self.0.get_mut(child.index()) {
            list.push(parent);
        }
    }

    /// Time O(1) + O(k), no allocation.
    fn linked(&self, child: Ix) -> impl Iterator<Item = Ix> + '_ {
        self.0
            .get(child.index())
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .copied()
    }
}

/// One exact-size boxed slice per node.
///
/// - `link`: O(k): `into_vec`, push, `into_boxed_slice`. With no spare
///   capacity, every link reallocates (and the shrink back to exact size
///   may reallocate again).
/// - `linked`: O(1) index + O(k) iteration.
/// - Live heap: 16 B x V for headers plus exactly 4 B x k per shared node.
///
/// Teaches: a smaller header and no slack save memory, but buying exactness
/// costs allocation churn on every edit.
pub struct BoxStore(Vec<Box<[Ix]>>);

impl ParentStore for BoxStore {
    const NAME: &'static str = "BoxStore";

    /// Time O(V) (writes V empty boxed slices, which do not allocate), space
    /// 16 B x V.
    fn with_nodes(n: usize) -> Self {
        let mut v = Vec::with_capacity(n);
        v.resize_with(n, Box::default);
        BoxStore(v)
    }

    /// Time O(k): the slice is moved into a `Vec`, grown by one, and shrunk
    /// back to an exact box (one or two reallocations).
    fn link(&mut self, child: Ix, parent: Ix) {
        if let Some(slot) = self.0.get_mut(child.index()) {
            let mut list = std::mem::take(slot).into_vec();
            list.push(parent);
            *slot = list.into_boxed_slice();
        }
    }

    /// Time O(1) + O(k), no allocation.
    fn linked(&self, child: Ix) -> impl Iterator<Item = Ix> + '_ {
        self.0
            .get(child.index())
            .map_or(&[][..], |b| &b[..])
            .iter()
            .copied()
    }
}

/// A sparse ordered map that holds shared nodes only.
///
/// - `link`: O(log S) to find or insert the entry, then an O(1) amortized
///   push.
/// - `linked`: O(log S) + O(k); a miss (an unshared node) still pays the
///   O(log S) search.
/// - Live heap: O(S) B-tree nodes (each holds up to 11 entries) plus one
///   `Vec` payload per shared node. Unshared nodes cost 0 bytes.
/// - Iteration over the map is in key order (deterministic, D7); each list
///   keeps link order.
///
/// Teaches: sparsity wins memory when few nodes are shared, but every
/// lookup pays a tree search, and at high share rates the per-entry overhead
/// (key + 24 B `Vec` header + B-tree slack) exceeds the dense header.
pub struct SparseStore(BTreeMap<Ix, Vec<Ix>>);

impl ParentStore for SparseStore {
    const NAME: &'static str = "SparseStore";

    /// Time O(1), no allocation: `BTreeMap::new` does not allocate.
    fn with_nodes(_n: usize) -> Self {
        SparseStore(BTreeMap::new())
    }

    /// Time O(log S) + O(1) amortized.
    fn link(&mut self, child: Ix, parent: Ix) {
        self.0.entry(child).or_default().push(parent);
    }

    /// Time O(log S) + O(k), no allocation.
    fn linked(&self, child: Ix) -> impl Iterator<Item = Ix> + '_ {
        self.0
            .get(&child)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .copied()
    }
}

/// Builds a store of type `S` for `n` nodes and applies `links` in order.
///
/// Time O(V) for `with_nodes` plus L times the cost of `S::link`.
pub fn build<S: ParentStore>(n: usize, links: &[(Ix, Ix)]) -> S {
    let mut store = S::with_nodes(n);
    for &(child, parent) in links {
        store.link(child, parent);
    }
    store
}

/// A generated graph: an owner per node, plus a list of extra links.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    /// `owners[i]` is the owner of node `i` (always `< i`). `owners[0]` is
    /// the root's placeholder `Ix(0)`: the root has no owner.
    pub owners: Vec<Ix>,
    /// `(child, parent)` pairs, shuffled so links to one child interleave.
    pub links: Vec<(Ix, Ix)>,
}

/// `SplitMix64` (Steele, Lea, Flood 2014), the seeding generator from
/// `java.util.SplittableRandom`. Tiny, fast, and good enough for shaping
/// benchmark data; avoids a `rand` dependency.
struct SplitMix64(u64);

impl SplitMix64 {
    /// Next 64 random bits. Time O(1).
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound` (Lemire's multiply-shift; the bias is at
    /// most bound / 2^64, irrelevant here). `bound` must be > 0. Time O(1).
    fn below(&mut self, bound: usize) -> usize {
        let wide = u128::from(self.next_u64()) * (bound as u128);
        // The high 64 bits are < bound, which fits in usize.
        usize::try_from(wide >> 64).unwrap_or(0)
    }
}

/// Generates a deterministic DAG shape with `n` nodes, about `share_pct`% of
/// the non-root nodes shared, from `seed`.
///
/// - Node 0 is the root. Node `i >= 1` gets a uniform owner `j < i` (a random
///   recursive tree, expected depth O(log V)).
/// - `round(share_pct% x (n - 1))` nodes are shared, picked uniformly from
///   `2..n` (node 1 cannot be shared: its only lower node is its owner, the
///   root). The shared count is therefore within 1 of the target.
/// - A shared node `i` gets `min(1..=3 uniform, i - 1)` distinct linked
///   parents `p < i`, `p != owner(i)`. Every edge goes from a lower to a
///   higher index, so the graph is acyclic and every link is valid for
///   `AttackTree::link` when all nodes are gates.
/// - `links` is Fisher-Yates shuffled so links to the same child interleave,
///   as in real editing; this exercises `BoxStore` reallocs and `BTreeMap`
///   insert order.
///
/// Same arguments, same `Shape` (P4). `share_pct` above 100 is treated as
/// 100, and `n` is clamped to `u32::MAX`.
///
/// Time O(V + L), space O(V + L), L <= 3 x V.
///
/// Teaches: benchmark data must be reproducible and valid for the real
/// model, or the numbers cannot be compared or trusted.
pub fn shape(n: usize, share_pct: u32, seed: u64) -> Shape {
    let n = n.min(u32::MAX as usize);
    let mut rng = SplitMix64(seed);

    let mut owners = Vec::with_capacity(n);
    if n > 0 {
        owners.push(Ix(0));
    }
    for i in 1..n {
        owners.push(Ix::from_usize(rng.below(i)));
    }

    let target = shared_target(n, share_pct);
    // Candidates for sharing: 2..n. Partial Fisher-Yates picks `target` of
    // them uniformly; then sort so parents are drawn in index order, which
    // keeps the draw sequence independent of the pick order.
    let mut candidates: Vec<usize> = (2..n.max(2)).collect();
    let picks = target.min(candidates.len());
    for t in 0..picks {
        let r = t + rng.below(candidates.len() - t);
        candidates.swap(t, r);
    }
    candidates.truncate(picks);
    candidates.sort_unstable();

    let mut links = Vec::with_capacity(picks * 3);
    for &i in &candidates {
        let owner = owners.get(i).map_or(0, |o| o.index());
        // i - 1 nodes below i are not the owner.
        let k = (1 + rng.below(3)).min(i - 1);
        let mut chosen: [usize; 3] = [usize::MAX; 3];
        let mut got = 0;
        while got < k {
            // Uniform over 0..i without the owner.
            let r = rng.below(i - 1);
            let p = if r >= owner { r + 1 } else { r };
            if !chosen[..got].contains(&p) {
                chosen[got] = p;
                got += 1;
            }
        }
        for &p in &chosen[..k] {
            links.push((Ix::from_usize(i), Ix::from_usize(p)));
        }
    }

    // Fisher-Yates (Durstenfeld) shuffle.
    for t in (1..links.len()).rev() {
        let r = rng.below(t + 1);
        links.swap(t, r);
    }

    Shape { owners, links }
}

/// The number of shared nodes `shape` aims for: `share_pct`% of the `n - 1`
/// non-root nodes, rounded to nearest. Time O(1).
pub fn shared_target(n: usize, share_pct: u32) -> usize {
    let pct = share_pct.min(100) as usize;
    (n.saturating_sub(1) * pct + 50) / 100
}

/// One benchmark case: a reference size and a share rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Case {
    /// Reference project name (`large` or `stress`).
    pub name: &'static str,
    /// Nodes, V.
    pub nodes: usize,
    /// Percentage of non-root nodes that are shared.
    pub share_pct: u32,
}

impl fmt::Display for Case {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}%", self.name, self.share_pct)
    }
}

/// Fixed seed for every case, so all runs measure the same graphs.
#[allow(dead_code)] // The test file uses its own seeds.
pub const SEED: u64 = 0x0A11_CE5E_ED00_0004;

/// The six cases: sizes {`large` 100,000, `stress` 1,000,000} x share
/// {1, 10, 50}%, sorted by size then share.
#[allow(dead_code)] // Used by the benches, not by the test file.
pub const CASES: [Case; 6] = [
    Case {
        name: "large",
        nodes: 100_000,
        share_pct: 1,
    },
    Case {
        name: "large",
        nodes: 100_000,
        share_pct: 10,
    },
    Case {
        name: "large",
        nodes: 100_000,
        share_pct: 50,
    },
    Case {
        name: "stress",
        nodes: 1_000_000,
        share_pct: 1,
    },
    Case {
        name: "stress",
        nodes: 1_000_000,
        share_pct: 10,
    },
    Case {
        name: "stress",
        nodes: 1_000_000,
        share_pct: 50,
    },
];
