//! Explain-why traces through the public API: the recorded `Derivation` of
//! every node for both built-in domains, ties, the default `*_kept` bodies,
//! the fold state machine, `explain` contents and mismatch checks,
//! `why_path`, agreement with a naive recursive evaluator, determinism, and
//! a deep chain without recursion.
//!
//! The fixtures (three-level tree, ladder, random DAGs) are the same as in
//! `tests/eval.rs`, copied because each integration test is its own crate.

use std::cell::Cell;

use aegis_model::{
    AttackTree, AttributeDomain, Derivation, Evaluation, GateKind, Kept, MaxMin, MinTime,
    ModelError, Node, NodeId, NodeIdx, evaluate,
};

type Res = Result<(), ModelError>;

fn gate(tree: &mut AttackTree, parent: NodeIdx, kind: GateKind) -> Result<NodeIdx, ModelError> {
    tree.add_child(parent, NodeId::new_v7(), Node::gate(kind, "gate"))
}

fn step(tree: &mut AttackTree, parent: NodeIdx) -> Result<NodeIdx, ModelError> {
    tree.add_child(parent, NodeId::new_v7(), Node::step("step"))
}

/// Leaf values indexed by arena position; `None` for gates.
struct Leaves<V>(Vec<Option<V>>);

impl<V: Clone> Leaves<V> {
    fn new(tree: &AttackTree) -> Self {
        Leaves(vec![None; tree.len()])
    }

    fn set(&mut self, idx: NodeIdx, v: V) {
        self.0[idx.index()] = Some(v);
    }

    fn get(&self, idx: NodeIdx) -> Option<V> {
        self.0.get(idx.index()).cloned().flatten()
    }
}

fn sel(rule: GateKind, child: NodeIdx) -> Option<Derivation> {
    Some(Derivation::Selected { rule, child })
}

fn comb(rule: GateKind) -> Option<Derivation> {
    Some(Derivation::Combined { rule })
}

/// Root OR over an AND and a SAND, each over three steps.
struct ThreeLevel {
    tree: AttackTree,
    and: NodeIdx,
    sand: NodeIdx,
    and_steps: [NodeIdx; 3],
    sand_steps: [NodeIdx; 3],
}

fn three_level() -> Result<ThreeLevel, ModelError> {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let r = tree.root();
    let and = gate(&mut tree, r, GateKind::And)?;
    let sand = gate(&mut tree, r, GateKind::SeqAnd)?;
    let and_steps = [
        step(&mut tree, and)?,
        step(&mut tree, and)?,
        step(&mut tree, and)?,
    ];
    let sand_steps = [
        step(&mut tree, sand)?,
        step(&mut tree, sand)?,
        step(&mut tree, sand)?,
    ];
    Ok(ThreeLevel {
        tree,
        and,
        sand,
        and_steps,
        sand_steps,
    })
}

/// AC1.
#[test]
fn maxmin_derivations() -> Res {
    let t = three_level()?;
    let mut leaves = Leaves::new(&t.tree);
    for (s, v) in t.and_steps.iter().zip([3_u8, 1, 4]) {
        leaves.set(*s, v);
    }
    for (s, v) in t.sand_steps.iter().zip([5_u8, 2, 6]) {
        leaves.set(*s, v);
    }
    let eval = evaluate(&t.tree, &MaxMin::<u8>::new(), |i| leaves.get(i))?;

    for s in t.and_steps.iter().chain(&t.sand_steps) {
        assert_eq!(eval.derivation(*s), Some(Derivation::Leaf));
    }
    // min(3, 1, 4) from the second step; min(5, 2, 6) from the second step.
    assert_eq!(eval.derivation(t.and), sel(GateKind::And, t.and_steps[1]));
    assert_eq!(
        eval.derivation(t.sand),
        sel(GateKind::SeqAnd, t.sand_steps[1])
    );
    // max(1, 2) from the SAND.
    assert_eq!(eval.derivation(t.tree.root()), sel(GateKind::Or, t.sand));
    assert_eq!(*eval.root_value(), 2);
    Ok(())
}

/// AC2: AND and SAND over the same three steps (SAND's children are links).
#[test]
fn min_time_derivations() -> Res {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let r = tree.root();
    let and = gate(&mut tree, r, GateKind::And)?;
    let sand = gate(&mut tree, r, GateKind::SeqAnd)?;
    let steps = [
        step(&mut tree, and)?,
        step(&mut tree, and)?,
        step(&mut tree, and)?,
    ];
    for s in steps {
        tree.link(sand, s)?;
    }
    let mut leaves = Leaves::new(&tree);
    for (s, v) in steps.iter().zip([4_u64, 10, 3]) {
        leaves.set(*s, v);
    }
    let eval = evaluate(&tree, &MinTime, |i| leaves.get(i))?;

    assert_eq!(eval.derivation(and), sel(GateKind::And, steps[1])); // slowest
    assert_eq!(eval.derivation(sand), comb(GateKind::SeqAnd)); // 4 + 10 + 3
    assert_eq!(eval.derivation(r), sel(GateKind::Or, and)); // min(10, 17)
    for s in steps {
        assert_eq!(eval.derivation(s), Some(Derivation::Leaf));
    }
    assert_eq!(eval.get(sand), Some(&17));
    Ok(())
}

/// AC3: the earlier child in stored order wins a tie, also when it was
/// added by `link`.
#[test]
fn ties_pick_first_child() -> Res {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    let r = tree.root();
    let h = gate(&mut tree, r, GateKind::And)?;
    let b = step(&mut tree, h)?;
    let g = gate(&mut tree, r, GateKind::Or)?;
    let a = step(&mut tree, g)?;
    tree.link(g, b)?;
    let c = step(&mut tree, g)?;
    let k = gate(&mut tree, r, GateKind::And)?;
    let x = step(&mut tree, k)?;
    let y = step(&mut tree, k)?;
    let q = gate(&mut tree, r, GateKind::SeqAnd)?;
    let z = step(&mut tree, q)?;
    let w = step(&mut tree, q)?;
    assert_eq!(tree.children(g), &[a, b, c]);
    assert_eq!(tree.children(r), &[h, g, k, q]);

    let mut levels = Leaves::new(&tree);
    for (n, v) in [(a, 1_u8), (b, 3), (c, 3), (x, 3), (y, 3), (z, 3), (w, 3)] {
        levels.set(n, v);
    }
    let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| levels.get(i))?;
    assert_eq!(eval.derivation(g), sel(GateKind::Or, b)); // linked b before c
    assert_eq!(eval.derivation(h), sel(GateKind::And, b));
    assert_eq!(eval.derivation(k), sel(GateKind::And, x));
    assert_eq!(eval.derivation(q), sel(GateKind::SeqAnd, z));
    assert_eq!(eval.derivation(r), sel(GateKind::And, h)); // all four equal

    let mut times = Leaves::new(&tree);
    for (n, v) in [(a, 5_u64), (b, 1), (c, 1), (x, 2), (y, 2), (z, 1), (w, 1)] {
        times.set(n, v);
    }
    let eval = evaluate(&tree, &MinTime, |i| times.get(i))?;
    assert_eq!(eval.derivation(g), sel(GateKind::Or, b)); // min tie: linked b
    assert_eq!(eval.derivation(k), sel(GateKind::And, x)); // max tie
    assert_eq!(eval.derivation(q), comb(GateKind::SeqAnd));
    // max(1, 1, 2, 2): k and q tie, k is first.
    assert_eq!(eval.derivation(r), sel(GateKind::And, k));
    Ok(())
}

/// Minimum cost with only the required methods: OR = cheapest, AND and
/// SAND = sum. Counts its calls.
struct MinCost {
    calls: Cell<usize>,
}

impl AttributeDomain for MinCost {
    type Value = u64;
    fn or(&self, acc: u64, next: &u64) -> u64 {
        self.calls.set(self.calls.get() + 1);
        acc.min(*next)
    }
    fn and(&self, acc: u64, next: &u64) -> u64 {
        self.calls.set(self.calls.get() + 1);
        acc.saturating_add(*next)
    }
}

fn min_cost_rule(kind: GateKind, vals: &[u64]) -> (u64, Option<usize>) {
    let value = match kind {
        GateKind::Or => vals.iter().copied().min().unwrap_or(0),
        GateKind::And | GateKind::SeqAnd => vals.iter().fold(0_u64, |a, v| a.saturating_add(*v)),
    };
    // The default `*_kept` never names a child, so only one-child gates
    // are `Selected`.
    (value, (vals.len() == 1).then_some(0))
}

/// AC4.
#[test]
fn default_kept_is_combined() -> Res {
    for (k, tree) in fixtures()?.iter().enumerate() {
        let cost = |i: NodeIdx| Some(((i.index() * 5 + k) % 9) as u64);
        let domain = MinCost {
            calls: Cell::new(0),
        };
        let eval = evaluate(tree, &domain, cost)?;
        let mut gates = 0;
        let mut edges = 0;
        for (i, _, node) in tree.iter() {
            let n = tree.children(i).len();
            let want = match node.gate_kind() {
                None => Some(Derivation::Leaf),
                Some(rule) if n == 1 => sel(rule, tree.children(i)[0]),
                Some(rule) => comb(rule),
            };
            assert_eq!(eval.derivation(i), want, "node {}", i.index());
            if node.gate_kind().is_some() {
                gates += 1;
                edges += n;
            }
        }
        assert_matches_naive(tree, &eval, &cost, &min_cost_rule);
        // The default calls the plain method exactly once per fold step.
        assert_eq!(domain.calls.get(), edges - gates);
    }
    Ok(())
}

/// Answers a scripted sequence of `Kept` tags: `Acc` keeps `acc`, `Next`
/// takes `next`, `Both` adds them, so the value always fits the tag.
struct Scripted {
    script: &'static [Kept],
    pos: Cell<usize>,
}

impl Scripted {
    fn step(&self, acc: u32, next: &u32) -> (u32, Kept) {
        let kept = self
            .script
            .get(self.pos.get())
            .copied()
            .unwrap_or(Kept::Both);
        self.pos.set(self.pos.get() + 1);
        let value = match kept {
            Kept::Acc => acc,
            Kept::Next => *next,
            Kept::Both => acc + next,
        };
        (value, kept)
    }
}

impl AttributeDomain for Scripted {
    type Value = u32;
    fn or(&self, acc: u32, next: &u32) -> u32 {
        self.step(acc, next).0
    }
    fn and(&self, acc: u32, next: &u32) -> u32 {
        self.step(acc, next).0
    }
    fn or_kept(&self, acc: u32, next: &u32) -> (u32, Kept) {
        self.step(acc, next)
    }
    fn and_kept(&self, acc: u32, next: &u32) -> (u32, Kept) {
        self.step(acc, next)
    }
    fn seq_and_kept(&self, acc: u32, next: &u32) -> (u32, Kept) {
        self.step(acc, next)
    }
}

/// AC5: one gate over `script.len() + 1` steps; `want` is the expected
/// deciding child (1-based), or `None` for `Combined`.
#[test]
fn kept_state_machine() -> Res {
    use Kept::{Acc, Both, Next};
    let cases: [(&'static [Kept], Option<usize>, u32); 9] = [
        (&[], Some(1), 1),
        (&[Acc], Some(1), 1),
        (&[Next], Some(2), 10),
        (&[Both], None, 11),
        (&[Both, Acc], None, 11),
        (&[Next, Acc], Some(2), 10),
        (&[Acc, Next, Both, Next], Some(5), 10_000),
        (&[Next, Both, Acc, Acc], None, 110),
        (&[Both, Both, Next], Some(4), 1_000),
    ];
    for kind in [GateKind::Or, GateKind::And, GateKind::SeqAnd] {
        for (script, want, value) in cases {
            let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(kind, "root"))?;
            let r = tree.root();
            let mut steps = Vec::new();
            for _ in 0..=script.len() {
                steps.push(step(&mut tree, r)?);
            }
            // Distinct powers of ten, so the value shows which inputs it used.
            let mut leaves = Leaves::new(&tree);
            for (n, s) in steps.iter().enumerate() {
                leaves.set(*s, 10_u32.pow(u32::try_from(n).unwrap_or(0)));
            }
            let domain = Scripted {
                script,
                pos: Cell::new(0),
            };
            let eval = evaluate(&tree, &domain, |i| leaves.get(i))?;
            assert_eq!(domain.pos.get(), script.len(), "{script:?}");
            let expected = match want {
                Some(n) => sel(kind, steps[n - 1]),
                None => comb(kind),
            };
            assert_eq!(eval.derivation(r), expected, "{kind:?} {script:?}");
            assert_eq!(*eval.root_value(), value, "{kind:?} {script:?}");
        }
    }
    Ok(())
}

/// AC7.
#[test]
fn explain_contents() -> Res {
    let t = three_level()?;
    let r = t.tree.root();
    let mut leaves = Leaves::new(&t.tree);
    for (s, v) in t.and_steps.iter().zip([3_u8, 1, 4]) {
        leaves.set(*s, v);
    }
    for (s, v) in t.sand_steps.iter().zip([5_u8, 2, 6]) {
        leaves.set(*s, v);
    }
    let eval = evaluate(&t.tree, &MaxMin::<u8>::new(), |i| leaves.get(i))?;

    let why = eval.explain(&t.tree, r).ok_or(ModelError::UnknownNode(r))?;
    assert_eq!(why.node(), r);
    assert_eq!(*why.value(), 2);
    assert_eq!(why.rule(), Some(GateKind::Or));
    assert_eq!(why.decisive(), Some(t.sand));
    assert_eq!(Some(why.derivation()), sel(GateKind::Or, t.sand));
    let inputs: Vec<(NodeIdx, u8)> = why.inputs().map(|(i, v)| (i, *v)).collect();
    assert_eq!(inputs, [(t.and, 1), (t.sand, 2)]);

    let why = eval
        .explain(&t.tree, t.and)
        .ok_or(ModelError::UnknownNode(t.and))?;
    assert_eq!(why.rule(), Some(GateKind::And));
    assert_eq!(why.decisive(), Some(t.and_steps[1]));
    let inputs: Vec<(NodeIdx, u8)> = why.inputs().map(|(i, v)| (i, *v)).collect();
    let want: Vec<(NodeIdx, u8)> = t.and_steps.into_iter().zip([3, 1, 4]).collect();
    assert_eq!(inputs, want);

    let leaf = t.sand_steps[2];
    let why = eval
        .explain(&t.tree, leaf)
        .ok_or(ModelError::UnknownNode(leaf))?;
    assert_eq!(why.node(), leaf);
    assert_eq!(*why.value(), 6);
    assert_eq!(why.derivation(), Derivation::Leaf);
    assert_eq!(why.rule(), None);
    assert_eq!(why.decisive(), None);
    assert_eq!(why.inputs().count(), 0);

    // A combined gate lists every input and names no deciding child.
    let eval = evaluate(&t.tree, &MinTime, |i| {
        leaves.get(i).map(|v| u64::from(v) * 10)
    })?;
    let why = eval
        .explain(&t.tree, t.sand)
        .ok_or(ModelError::UnknownNode(t.sand))?;
    assert_eq!(*why.value(), 130);
    assert_eq!(
        why.derivation(),
        Derivation::Combined {
            rule: GateKind::SeqAnd
        }
    );
    assert_eq!(why.rule(), Some(GateKind::SeqAnd));
    assert_eq!(why.decisive(), None);
    let inputs: Vec<(NodeIdx, u64)> = why.inputs().map(|(i, v)| (i, *v)).collect();
    let want: Vec<(NodeIdx, u64)> = t.sand_steps.into_iter().zip([50, 20, 60]).collect();
    assert_eq!(inputs, want);
    // The explanation outlives a temporary borrow of itself.
    let total: u64 = eval
        .explain(&t.tree, t.sand)
        .into_iter()
        .flat_map(|e| e.inputs())
        .map(|(_, v)| *v)
        .sum();
    assert_eq!(total, 130);
    Ok(())
}

/// AC8.
#[test]
fn explain_rejects_mismatch() -> Res {
    // A: root OR > n1 AND > n2 step. B: root OR > n1 step, n2 step.
    // C: root AND > n1 AND > n2 step. Same node count, same indices.
    let mut a = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let a_root = a.root();
    let a1 = gate(&mut a, a_root, GateKind::And)?;
    let a2 = step(&mut a, a1)?;
    let mut b = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let b_root = b.root();
    let b1 = step(&mut b, b_root)?;
    let b2 = step(&mut b, b_root)?;
    let mut c = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    let c_root = c.root();
    let c1 = gate(&mut c, c_root, GateKind::And)?;
    step(&mut c, c1)?;
    assert_eq!((a1, a2), (b1, b2));

    let eval_a = evaluate(&a, &MaxMin::<u8>::new(), |_| Some(1))?;
    let eval_b = evaluate(&b, &MaxMin::<u8>::new(), |_| Some(1))?;
    // Matching tree: every node explains.
    for (i, _, _) in a.iter() {
        assert!(eval_a.explain(&a, i).is_some(), "node {}", i.index());
    }
    // A gate recorded where the tree has a step.
    assert!(eval_a.explain(&b, a1).is_none());
    // A leaf recorded where the tree has a gate.
    assert!(eval_b.explain(&a, b1).is_none());
    // Same kinds at n2 (a step in both): still explains.
    assert!(eval_a.explain(&b, a2).is_some());
    // A different gate kind than the recorded rule.
    assert!(eval_a.explain(&c, a.root()).is_none());
    assert!(eval_a.explain(&c, a1).is_some()); // AND in both

    // An index outside the evaluation.
    let mut big = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let big_root = big.root();
    let mut far = big.root();
    for _ in 0..5 {
        far = step(&mut big, big_root)?;
    }
    assert!(eval_a.explain(&big, far).is_none());
    assert!(eval_a.explain(&a, far).is_none());
    assert_eq!(eval_a.derivation(far), None);

    // An edited tree (one more node) is rejected outright.
    let mut edited = a.clone();
    step(&mut edited, a1)?;
    assert!(eval_a.explain(&edited, a.root()).is_none());
    assert!(eval_a.explain(&edited, a2).is_none());
    Ok(())
}

/// AC9.
#[test]
fn why_path_follows_decisive() -> Res {
    // root OR > [g1 AND > [s, t], g2 AND > [u, s (link)]].
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let r = tree.root();
    let g1 = gate(&mut tree, r, GateKind::And)?;
    let s = step(&mut tree, g1)?;
    let t = step(&mut tree, g1)?;
    let g2 = gate(&mut tree, r, GateKind::And)?;
    let u = step(&mut tree, g2)?;
    tree.link(g2, s)?;
    assert_eq!(tree.owner(s), Some(g1));

    let mut leaves = Leaves::new(&tree);
    for (n, v) in [(s, 3_u8), (t, 1), (u, 4)] {
        leaves.set(n, v);
    }
    let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| leaves.get(i))?;
    // g1 = min(3, 1) = 1, g2 = min(4, 3) = 3 via the link, root = 3.
    let path: Vec<NodeIdx> = eval.why_path(r).collect();
    assert_eq!(path, [r, g2, s]);
    for pair in path.windows(2) {
        assert!(tree.children(pair[0]).contains(&pair[1]), "{pair:?}");
    }
    for n in &path {
        assert_eq!(eval.get(*n), Some(eval.root_value()));
    }
    assert_eq!(eval.derivation(s), Some(Derivation::Leaf));
    // From a node in the middle, and from a leaf.
    assert_eq!(eval.why_path(g1).collect::<Vec<_>>(), [g1, t]);
    assert_eq!(eval.why_path(u).collect::<Vec<_>>(), [u]);

    // MinTime: stops at the SAND, which combines its children.
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let r = tree.root();
    let sand = gate(&mut tree, r, GateKind::SeqAnd)?;
    let a = step(&mut tree, sand)?;
    let b = step(&mut tree, sand)?;
    let c = step(&mut tree, r)?;
    let mut times = Leaves::new(&tree);
    for (n, v) in [(a, 1_u64), (b, 2), (c, 10)] {
        times.set(n, v);
    }
    let eval = evaluate(&tree, &MinTime, |i| times.get(i))?;
    assert_eq!(eval.why_path(r).collect::<Vec<_>>(), [r, sand]);
    assert_eq!(eval.derivation(sand), comb(GateKind::SeqAnd));

    // Outside the evaluation: empty.
    let mut big = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let big_root = big.root();
    let mut far = big.root();
    for _ in 0..10 {
        far = step(&mut big, big_root)?;
    }
    assert_eq!(eval.why_path(far).count(), 0);
    Ok(())
}

/// Ladder DAG: the root over two gates per level, each gate over both gates
/// of the next level, and the last level over two shared steps.
fn ladder(levels: usize) -> Result<AttackTree, ModelError> {
    const KINDS: [GateKind; 3] = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let r = tree.root();
    let mut prev = [gate(&mut tree, r, KINDS[0])?, gate(&mut tree, r, KINDS[0])?];
    for level in 1..levels {
        let kind = KINDS[level % KINDS.len()];
        let next = [
            gate(&mut tree, prev[0], kind)?,
            gate(&mut tree, prev[1], kind)?,
        ];
        tree.link(prev[0], next[1])?;
        tree.link(prev[1], next[0])?;
        prev = next;
    }
    let s = [step(&mut tree, prev[0])?, step(&mut tree, prev[1])?];
    tree.link(prev[0], s[1])?;
    tree.link(prev[1], s[0])?;
    Ok(tree)
}

/// Deterministic SplitMix64, so fixtures are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n as u64)).unwrap_or(0)
    }
}

/// A random DAG of `n` nodes with mixed gate kinds (same as `tests/eval.rs`).
fn random_dag(n: usize, links: usize, seed: u64) -> Result<AttackTree, ModelError> {
    const KINDS: [GateKind; 3] = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
    let mut rng = Rng(seed);
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let mut gates = vec![tree.root()];
    let mut childless = vec![tree.root()];
    while tree.len() < n {
        let parent = match childless.pop() {
            Some(g) => g,
            None => gates[rng.below(gates.len())],
        };
        let room = n - tree.len() - childless.len();
        if room > 1 && rng.below(10) < 4 {
            let g = gate(&mut tree, parent, KINDS[rng.below(3)])?;
            gates.push(g);
            childless.push(g);
        } else {
            step(&mut tree, parent)?;
        }
    }
    let all: Vec<NodeIdx> = tree.iter().map(|(i, _, _)| i).collect();
    for _ in 0..links {
        let p = gates[rng.below(gates.len())];
        let c = all[rng.below(all.len())];
        if c > p && !tree.children(p).contains(&c) {
            tree.link(p, c)?;
        }
    }
    Ok(tree)
}

/// The 005 fixtures (same as `fixtures` in `tests/eval.rs`).
fn fixtures() -> Result<Vec<AttackTree>, ModelError> {
    let mut out = vec![
        AttackTree::new(NodeId::new_v7(), Node::step("only"))?,
        three_level()?.tree,
        ladder(6)?,
    ];
    let mut t = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    let r = t.root();
    let or = gate(&mut t, r, GateKind::Or)?;
    let sand = gate(&mut t, r, GateKind::SeqAnd)?;
    let shared = gate(&mut t, or, GateKind::And)?;
    step(&mut t, shared)?;
    step(&mut t, shared)?;
    step(&mut t, or)?;
    step(&mut t, sand)?;
    t.link(sand, shared)?;
    t.link(r, shared)?;
    out.push(t);
    for seed in 1..=4 {
        out.push(random_dag(200, 30, seed)?);
    }
    Ok(out)
}

/// A gate rule for the naive evaluator: the gate's value from its
/// children's values, and the position of the deciding child (`None` when
/// several children are combined).
type Rule<V> = dyn Fn(GateKind, &[V]) -> (V, Option<usize>);

/// Naive recursive evaluation of one node with its derivation, written from
/// the domain definitions without the crate's fold. Exponential on DAGs;
/// fine for the small fixtures.
fn naive<V: Clone>(
    tree: &AttackTree,
    idx: NodeIdx,
    leaf: &dyn Fn(NodeIdx) -> Option<V>,
    rule: &Rule<V>,
) -> Option<(V, Derivation)> {
    let node = tree.get(idx)?;
    let Some(kind) = node.gate_kind() else {
        return leaf(idx).map(|v| (v, Derivation::Leaf));
    };
    let children = tree.children(idx);
    let vals: Option<Vec<V>> = children
        .iter()
        .map(|c| naive(tree, *c, leaf, rule).map(|(v, _)| v))
        .collect();
    let vals = vals?;
    if vals.is_empty() {
        return None;
    }
    let (value, pos) = rule(kind, &vals);
    let how = match pos {
        Some(p) => Derivation::Selected {
            rule: kind,
            child: *children.get(p)?,
        },
        None => Derivation::Combined { rule: kind },
    };
    Some((value, how))
}

/// Position of the first child equal to `target`: the earliest on a tie.
fn first_eq<V: PartialEq>(vals: &[V], target: Option<&V>) -> Option<usize> {
    let target = target?;
    vals.iter().position(|v| v == target)
}

fn maxmin_rule(kind: GateKind, vals: &[u8]) -> (u8, Option<usize>) {
    let best = match kind {
        GateKind::Or => vals.iter().max(),
        GateKind::And | GateKind::SeqAnd => vals.iter().min(),
    };
    (best.copied().unwrap_or(0), first_eq(vals, best))
}

fn min_time_rule(kind: GateKind, vals: &[u64]) -> (u64, Option<usize>) {
    match kind {
        GateKind::Or => {
            let best = vals.iter().min();
            (best.copied().unwrap_or(0), first_eq(vals, best))
        }
        GateKind::And => {
            let best = vals.iter().max();
            (best.copied().unwrap_or(0), first_eq(vals, best))
        }
        GateKind::SeqAnd => {
            let sum = vals
                .iter()
                .try_fold(0_u64, |a, v| a.checked_add(*v))
                .unwrap_or(u64::MAX);
            (sum, (vals.len() == 1).then_some(0))
        }
    }
}

fn assert_matches_naive<V: Clone + PartialEq + std::fmt::Debug>(
    tree: &AttackTree,
    eval: &Evaluation<V>,
    leaf: &dyn Fn(NodeIdx) -> Option<V>,
    rule: &Rule<V>,
) {
    assert_eq!(eval.len(), tree.len());
    for (i, _, _) in tree.iter() {
        let want = naive(tree, i, leaf, rule);
        let got = eval.get(i).cloned().zip(eval.derivation(i));
        assert_eq!(got, want, "node {}", i.index());
    }
}

/// AC10.
#[test]
fn matches_naive_trace() -> Res {
    for (k, tree) in fixtures()?.iter().enumerate() {
        let shift = k as u64;
        // Few levels, so ties are common.
        let level = |i: NodeIdx| Some(u8::try_from((i.index() * 7 + k) % 5).unwrap_or(0));
        let eval = evaluate(tree, &MaxMin::<u8>::new(), level)?;
        assert_matches_naive(tree, &eval, &level, &maxmin_rule);

        let time = |i: NodeIdx| {
            let i = i.index() as u64;
            Some(if (i + shift).is_multiple_of(11) {
                u64::MAX / 2
            } else {
                (i * 13 + shift) % 100
            })
        };
        let eval = evaluate(tree, &MinTime, time)?;
        assert_matches_naive(tree, &eval, &time, &min_time_rule);

        // Every why_path is a parent-to-child chain that ends at a leaf or
        // at a combined gate.
        for (i, _, _) in tree.iter() {
            let path: Vec<NodeIdx> = eval.why_path(i).collect();
            assert_eq!(path.first(), Some(&i));
            for pair in path.windows(2) {
                assert!(tree.children(pair[0]).contains(&pair[1]));
            }
            let last = path.last().and_then(|n| eval.derivation(*n));
            assert!(
                matches!(last, Some(Derivation::Leaf | Derivation::Combined { .. })),
                "{last:?}"
            );
        }
    }
    Ok(())
}

/// AC11.
#[test]
fn trace_deterministic_and_deep() -> Res {
    for tree in fixtures()? {
        let level = |i: NodeIdx| Some(u8::try_from(i.index() % 3).unwrap_or(0));
        let first = evaluate(&tree, &MaxMin::<u8>::new(), level)?;
        let second = evaluate(&tree, &MaxMin::<u8>::new(), level)?;
        let copy = tree.clone();
        let third = evaluate(&copy, &MaxMin::<u8>::new(), level)?;
        assert_eq!(first, second);
        assert_eq!(first, third);
        assert_eq!(format!("{first:?}"), format!("{third:?}"));
        for (i, _, _) in tree.iter() {
            assert_eq!(first.derivation(i), third.derivation(i));
            assert!(first.derivation(i).is_some());
            assert!(first.why_path(i).eq(third.why_path(i)));
        }

        let time = |i: NodeIdx| Some((i.index() % 4) as u64);
        let a = evaluate(&tree, &MinTime, time)?;
        let b = evaluate(&copy, &MinTime, time)?;
        assert_eq!(a, b);
    }

    // A 100,000-deep chain of one-child gates: every gate selects its only
    // child, so the path runs from the root to the step.
    const KINDS: [GateKind; 3] = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
    const DEPTH: usize = 100_000;
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "g"))?;
    let mut last = tree.root();
    for k in 1..DEPTH {
        last = gate(&mut tree, last, KINDS[k % KINDS.len()])?;
    }
    let s = step(&mut tree, last)?;
    let eval = evaluate(&tree, &MinTime, |i| (i == s).then_some(42))?;
    assert_eq!(eval.why_path(tree.root()).count(), DEPTH + 1);
    assert_eq!(eval.why_path(tree.root()).last(), Some(s));
    let last_kind = KINDS[(DEPTH - 1) % KINDS.len()];
    assert_eq!(eval.derivation(last), sel(last_kind, s));
    assert_eq!(
        eval.derivation(tree.root()),
        sel(GateKind::And, tree.children(tree.root())[0])
    );
    Ok(())
}

#[test]
fn top_k_attack_paths_orders_and_explains_bottlenecks() -> Res {
    let t = three_level()?;
    let mut leaves = Leaves::new(&t.tree);
    for (s, v) in t.and_steps.iter().zip([3_u8, 1, 4]) {
        leaves.set(*s, v);
    }
    for (s, v) in t.sand_steps.iter().zip([5_u8, 2, 6]) {
        leaves.set(*s, v);
    }
    let eval = evaluate(&t.tree, &MaxMin::<u8>::new(), |i| leaves.get(i))?;

    assert_eq!(eval.top_k_paths(&t.tree, 0), Some(vec![]));
    let top1 = eval
        .top_k_paths(&t.tree, 1)
        .ok_or(ModelError::UnknownNode(t.tree.root()))?;
    assert_eq!(top1.len(), 1);
    assert_eq!(top1[0].steps(), &t.sand_steps);
    assert_eq!(*top1[0].feasibility(), 2);
    assert_eq!(top1[0].bottleneck(), t.sand_steps[1]);
    assert_eq!(top1[0].len(), 3);
    assert!(!top1[0].is_empty());
    assert!(top1[0].contains(t.sand_steps[0]));
    assert!(!top1[0].contains(t.and_steps[0]));

    let all = eval
        .top_k_paths(&t.tree, 10)
        .ok_or(ModelError::UnknownNode(t.tree.root()))?;
    assert_eq!(all.len(), 2);
    assert_eq!(all[0], top1[0]);
    assert_eq!(all[1].steps(), &t.and_steps);
    assert_eq!(*all[1].feasibility(), 1);
    assert_eq!(all[1].bottleneck(), t.and_steps[1]);
    Ok(())
}

#[test]
fn top_k_attack_paths_lazy_on_exponential_and_of_or_tree() -> Res {
    // 20 independent binary OR gates under an AND root => 2^20 = 1,048,576
    // distinct minimal attack paths (61 nodes total). Materializing all paths
    // would allocate over a million vectors; lazy best-first top-5 expands
    // only a handful of states.
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    let r = tree.root();
    let mut hi_steps = Vec::new();
    for stage in 0..20_u8 {
        let g = gate(&mut tree, r, GateKind::Or)?;
        let s_hi = step(&mut tree, g)?;
        let s_lo = step(&mut tree, g)?;
        hi_steps.push((s_hi, s_lo, stage));
    }
    let mut leaves = Leaves::new(&tree);
    for &(s_hi, s_lo, stage) in &hi_steps {
        leaves.set(s_hi, 50_u8);
        leaves.set(s_lo, 40_u8.saturating_sub(stage));
    }
    let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| leaves.get(i))?;
    let top5 = eval
        .top_k_paths(&tree, 5)
        .ok_or(ModelError::UnknownNode(r))?;
    assert_eq!(top5.len(), 5);
    assert_eq!(*top5[0].feasibility(), 50);
    assert_eq!(top5[0].len(), 20);
    // #2 flips stage 0 to 40; #3 flips stage 1 to 39; #4 flips stages 0 and 1
    // (min(40, 39) == 39); #5 flips stage 2 to 38.
    assert_eq!(*top5[1].feasibility(), 40);
    assert_eq!(top5[1].bottleneck(), hi_steps[0].1);
    assert_eq!(*top5[2].feasibility(), 39);
    assert_eq!(top5[2].bottleneck(), hi_steps[1].1);
    assert!(top5[2].contains(hi_steps[0].0));
    assert_eq!(*top5[3].feasibility(), 39);
    assert_eq!(top5[3].bottleneck(), hi_steps[1].1);
    assert!(top5[3].contains(hi_steps[0].1));
    assert_eq!(*top5[4].feasibility(), 38);
    assert_eq!(top5[4].bottleneck(), hi_steps[2].1);
    Ok(())
}
