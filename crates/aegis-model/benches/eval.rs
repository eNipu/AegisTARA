//! Full evaluation time against the "full recompute" budget (engineering
//! standards section 2): at most 200 ms at `large` (100,000 nodes) and 3 s
//! at `stress` (1,000,000 nodes). Results are recorded in ADR 0005, and
//! the before/after cost of the explain-why trace (recorded by every
//! `evaluate` call) in ADR 0006.
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
use std::sync::{Arc, OnceLock};

use aegis_model::{
    AssetId, AttackTree, DamageId, FeasibilityLevel, GateKind, MaxMin, Methodology, ModelError,
    Node, NodeId, NodeIdx, Op, Project, ProjectEvalCache, Provenance, StepTarget, ThreatId,
    TreeEvalCache, TreeId, TreeRole, evaluate,
};
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
    steps: Vec<NodeIdx>,
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
    Ok(Fixture {
        tree,
        levels,
        steps,
    })
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

/// Incremental evaluation of a single step rating change via `TreeEvalCache`
/// against the "Incremental recompute on one rating change" budget (<= 5 ms
/// at `large` = 100,000 nodes).
#[divan::bench(args = SIZES, sample_count = 100, max_time = 10)]
fn incremental_single_step(bencher: Bencher, n: usize) {
    let Some(f) = fixture(n) else {
        return;
    };
    let domain = MaxMin::<u8>::new();
    let leaf = |i: NodeIdx| f.levels.get(i.index()).copied();
    let Ok(mut cache) = TreeEvalCache::new(&f.tree, &domain, leaf) else {
        return;
    };
    let mut cursor = 0usize;
    let mut alt_val = 0u8;
    bencher.bench_local(|| {
        let step = f.steps[cursor % f.steps.len()];
        cursor = cursor.wrapping_add(1);
        alt_val = (alt_val + 1) % 5;
        let v = alt_val;
        cache.recompute_steps(black_box(&f.tree), &domain, [step], |i| {
            if i == step {
                Some(v)
            } else {
                f.levels.get(i.index()).copied()
            }
        })
    });
}

// ---------------------------------------------------------------------------
// One rating edit inside a shared fragment (142e).
// ---------------------------------------------------------------------------

/// Fragments in the reference graph of `fragment_single_edit_50`.
const FRAGMENTS: usize = 50;
/// Threat trees that each reference three fragments.
const FRAGMENT_USERS: usize = 10;

/// A project to edit, its cache, the step the benchmark re-rates, and the
/// two levels it alternates between.
struct FragmentFixture {
    project: Project,
    cache: ProjectEvalCache,
    step: (TreeId, NodeId),
    levels: [FeasibilityLevel; 2],
}

/// Adds one OR-rooted tree with a step per entry of `rated` and a step per
/// entry of `refs`, each linked to that fragment. Returns the tree and its
/// rated steps.
fn add_ref_tree(
    project: &mut Project,
    role: TreeRole,
    rated: &[FeasibilityLevel],
    refs: &[TreeId],
) -> Option<(TreeId, Vec<NodeId>)> {
    let id = TreeId::new_v7();
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).ok()?;
    let mut steps = Vec::new();
    for &level in rated {
        let node = NodeId::new_v7();
        tree.add_child(tree.root(), node, Node::step("s")).ok()?;
        steps.push((node, level));
    }
    let mut ref_nodes = Vec::new();
    for _ in refs {
        let node = NodeId::new_v7();
        tree.add_child(tree.root(), node, Node::step("r")).ok()?;
        ref_nodes.push(node);
    }
    let rated_steps = steps.iter().map(|&(node, _)| node).collect();
    project
        .apply(Op::AddAttackTree {
            role,
            id,
            tree,
            step_feasibility: steps,
            provenance: Provenance::default(),
        })
        .ok()?;
    for (&node, &fragment) in ref_nodes.iter().zip(refs) {
        project
            .apply(Op::UseFragment {
                tree: id,
                node,
                fragment,
                provenance: Provenance::default(),
            })
            .ok()?;
    }
    Some((id, rated_steps))
}

/// 50 fragments in a binary reference tree that all also reference one
/// common sink fragment, so an edit in the sink fans out through nested
/// diamonds; then 10 threat trees referencing three fragments each, one
/// threat scenario per threat tree. Every own rating but the sink's is
/// `very-low`, so the sink decides every root above it.
fn fragment_fixture() -> Option<FragmentFixture> {
    let m = Arc::new(Methodology::iso21434_r155().ok()?);
    let (very_low, low, high) = (
        m.feasibility_level("very-low")?,
        m.feasibility_level("low")?,
        m.feasibility_level("high")?,
    );
    let mut project = Project::new(Arc::clone(&m));
    let mut ids: Vec<Option<TreeId>> = vec![None; FRAGMENTS];
    let mut step = None;
    // Bottom-up: children 2i+1 and 2i+2 of i are built before i.
    for i in (0..FRAGMENTS).rev() {
        let mut refs = Vec::new();
        for child in [2 * i + 1, 2 * i + 2] {
            if let Some(&Some(id)) = ids.get(child) {
                refs.push(id);
            }
        }
        if let Some(Some(sink)) = ids.last().filter(|_| i + 1 < FRAGMENTS) {
            refs.push(*sink);
        }
        let own: &[FeasibilityLevel] = if i + 1 == FRAGMENTS {
            &[very_low, high]
        } else {
            &[very_low, very_low]
        };
        let (id, rated) = add_ref_tree(&mut project, TreeRole::Fragment, own, &refs)?;
        if i + 1 == FRAGMENTS {
            step = Some((id, *rated.last()?));
        }
        ids[i] = Some(id);
    }
    let asset = AssetId::new_v7();
    let damage = DamageId::new_v7();
    let safety = m.impact_category("safety")?;
    let severe = m.level(safety, "severe")?;
    let category = m.threat_categories().next()?;
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Crash".into(),
            impacts: vec![(safety, severe)],
            provenance: Provenance::default(),
        },
    ] {
        project.apply(op).ok()?;
    }
    let roots: Vec<TreeId> = ids.iter().take(3).copied().collect::<Option<Vec<_>>>()?;
    for _ in 0..FRAGMENT_USERS {
        let (tree, _) = add_ref_tree(
            &mut project,
            TreeRole::Threat,
            &[very_low, very_low],
            &roots,
        )?;
        let threat = ThreatId::new_v7();
        for op in [
            Op::AddThreatScenario {
                id: threat,
                name: "Spoof".into(),
                asset,
                category,
                catalog_refs: vec![],
                damages: vec![damage],
                provenance: Provenance::default(),
            },
            Op::SetThreatAttackTree {
                threat,
                tree: Some(tree),
                provenance: Provenance::default(),
            },
        ] {
            project.apply(op).ok()?;
        }
    }
    let cache = ProjectEvalCache::new(&project);
    Some(FragmentFixture {
        project,
        cache,
        step: step?,
        levels: [high, low],
    })
}

/// One step rating edit inside a shared fragment, with the incremental cache
/// update that follows it, against the "recompute after one rating change"
/// budget (engineering standards section 2): at most 5 ms. The project is
/// built outside the timed region; each sample re-rates the same step with
/// the other level, so the sink's root really moves and every tree above it
/// is invalidated.
#[divan::bench(sample_count = 100, max_time = 10)]
fn fragment_single_edit_50(bencher: Bencher) {
    let Some(mut f) = fragment_fixture() else {
        eprintln!("fragment fixture could not be built");
        return;
    };
    let edit = |level: FeasibilityLevel| Op::SetStepFeasibility {
        target: StepTarget::Step(f.step.0, f.step.1),
        level: Some(level),
        provenance: Provenance::default(),
    };
    // The cost of one edit, measured outside the timed region.
    let (nodes, threats) = (f.cache.recomputed_nodes(), f.cache.recomputed_threats());
    if f.project
        .apply_cached(edit(f.levels[1]), &mut f.cache)
        .is_err()
    {
        eprintln!("fragment edit failed");
        return;
    }
    let (nodes, threats) = (
        f.cache.recomputed_nodes() - nodes,
        f.cache.recomputed_threats() - threats,
    );
    eprintln!(
        "fragment bench: {FRAGMENTS} fragments, {FRAGMENT_USERS} using trees, \
         {nodes} nodes and {threats} threats recomputed per edit"
    );
    let mut flip = 1_usize;
    bencher.counter(ItemsCount::new(nodes)).bench_local(|| {
        flip ^= 1;
        let level = f.levels[flip];
        f.project.apply_cached(black_box(edit(level)), &mut f.cache)
    });
}
