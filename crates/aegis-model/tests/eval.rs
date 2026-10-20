//! Tree evaluation through the public API: values per gate kind for both
//! built-in domains, fold order, the exactly-once guarantee on shared nodes,
//! linear cost on an exponential-path DAG, errors, determinism, agreement
//! with a naive recursive evaluator, and a deep chain without recursion.

use std::cell::Cell;

use aegis_model::{
    AttackTree, AttributeDomain, Evaluation, GateKind, MaxMin, MinTime, ModelError, Node, NodeId,
    NodeIdx, evaluate,
};

type Res = Result<(), ModelError>;

fn gate(tree: &mut AttackTree, parent: NodeIdx, kind: GateKind) -> Result<NodeIdx, ModelError> {
    tree.add_child(parent, NodeId::new_v7(), Node::gate(kind, "gate"))
}

fn step(tree: &mut AttackTree, parent: NodeIdx) -> Result<NodeIdx, ModelError> {
    tree.add_child(parent, NodeId::new_v7(), Node::step("step"))
}

fn id(tree: &AttackTree, idx: NodeIdx) -> Result<NodeId, ModelError> {
    tree.id_of(idx).ok_or(ModelError::UnknownNode(idx))
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

fn is_step(tree: &AttackTree, idx: NodeIdx) -> bool {
    tree.get(idx).is_some_and(|n| n.gate_kind().is_none())
}

/// Counts the domain calls per gate kind, then delegates.
struct Counting<D> {
    inner: D,
    or: Cell<usize>,
    and: Cell<usize>,
    seq: Cell<usize>,
}

impl<D> Counting<D> {
    fn new(inner: D) -> Self {
        Counting {
            inner,
            or: Cell::new(0),
            and: Cell::new(0),
            seq: Cell::new(0),
        }
    }

    fn total(&self) -> usize {
        self.or.get() + self.and.get() + self.seq.get()
    }
}

impl<D: AttributeDomain> AttributeDomain for Counting<D> {
    type Value = D::Value;
    fn or(&self, acc: D::Value, next: &D::Value) -> D::Value {
        self.or.set(self.or.get() + 1);
        self.inner.or(acc, next)
    }
    fn and(&self, acc: D::Value, next: &D::Value) -> D::Value {
        self.and.set(self.and.get() + 1);
        self.inner.and(acc, next)
    }
    fn seq_and(&self, acc: D::Value, next: &D::Value) -> D::Value {
        self.seq.set(self.seq.get() + 1);
        self.inner.seq_and(acc, next)
    }
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

#[test]
fn maxmin_values() -> Res {
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
        assert_eq!(eval.get(*s).copied(), leaves.get(*s));
    }
    assert_eq!(eval.get(t.and), Some(&1)); // min(3, 1, 4)
    assert_eq!(eval.get(t.sand), Some(&2)); // min(5, 2, 6)
    assert_eq!(eval.get(t.tree.root()), Some(&2)); // max(1, 2)
    assert_eq!(*eval.root_value(), 2);
    assert_eq!(eval.len(), t.tree.len());
    Ok(())
}

#[test]
fn min_time_values() -> Res {
    // AND and SAND over the same three steps (SAND's children are links).
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
    assert_eq!(eval.get(and), Some(&10)); // max: parallel
    assert_eq!(eval.get(sand), Some(&17)); // sum: sequential
    assert_eq!(*eval.root_value(), 10); // min
    for s in steps {
        assert_eq!(eval.get(s).copied(), leaves.get(s));
    }

    // Saturation instead of an overflow panic.
    leaves.set(steps[0], u64::MAX - 1);
    let eval = evaluate(&tree, &MinTime, |i| leaves.get(i))?;
    assert_eq!(eval.get(sand), Some(&u64::MAX));
    assert_eq!(eval.get(and), Some(&(u64::MAX - 1)));
    assert_eq!(*eval.root_value(), u64::MAX - 1);
    Ok(())
}

/// Records the fold as text: `|` for OR, `&` for AND, `>` for SAND.
struct Concat;

impl AttributeDomain for Concat {
    type Value = String;
    fn or(&self, acc: String, next: &String) -> String {
        format!("({acc}|{next})")
    }
    fn and(&self, acc: String, next: &String) -> String {
        format!("({acc}&{next})")
    }
    fn seq_and(&self, mut acc: String, next: &String) -> String {
        acc.push('>');
        acc.push_str(next);
        acc
    }
}

#[test]
fn seq_and_order() -> Res {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "root"))?;
    let r = tree.root();
    let x = gate(&mut tree, r, GateKind::Or)?;
    let c = step(&mut tree, x)?;
    let d = step(&mut tree, x)?;
    let a = step(&mut tree, r)?;
    let b = step(&mut tree, r)?;
    tree.link(r, c)?; // linked child goes last
    let y = gate(&mut tree, x, GateKind::And)?;
    tree.link(y, b)?;
    tree.link(y, a)?;
    assert_eq!(tree.children(r), &[x, a, b, c]);

    let mut leaves = Leaves::new(&tree);
    for (s, v) in [(a, "a"), (b, "b"), (c, "c"), (d, "d")] {
        leaves.set(s, v.to_string());
    }
    let eval = evaluate(&tree, &Concat, |i| leaves.get(i))?;
    assert_eq!(eval.get(y).map(String::as_str), Some("(b&a)"));
    assert_eq!(eval.get(x).map(String::as_str), Some("((c|d)|(b&a))"));
    assert_eq!(eval.root_value(), "((c|d)|(b&a))>a>b>c");
    Ok(())
}

#[test]
fn evaluates_each_node_once() -> Res {
    // Diamond: root AND over g1 and g2 (both OR); `s` is shared.
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    let r = tree.root();
    let g1 = gate(&mut tree, r, GateKind::Or)?;
    let g2 = gate(&mut tree, r, GateKind::Or)?;
    let s = step(&mut tree, g1)?;
    let t = step(&mut tree, g1)?;
    let u = step(&mut tree, g2)?;
    tree.link(g2, s)?;

    let mut leaves = Leaves::new(&tree);
    for (n, v) in [(s, 9_u8), (t, 1), (u, 2)] {
        leaves.set(n, v);
    }
    let calls: Vec<Cell<usize>> = (0..tree.len()).map(|_| Cell::new(0)).collect();
    let domain = Counting::new(MaxMin::<u8>::new());
    let eval = evaluate(&tree, &domain, |i| {
        calls[i.index()].set(calls[i.index()].get() + 1);
        leaves.get(i)
    })?;

    for (i, _, _) in tree.iter() {
        let want = usize::from(is_step(&tree, i));
        assert_eq!(calls[i.index()].get(), want, "leaf calls for {}", i.index());
    }
    let combines: usize = tree
        .iter()
        .filter(|(i, _, _)| !is_step(&tree, *i))
        .map(|(i, _, _)| tree.children(i).len() - 1)
        .sum();
    assert_eq!(combines, 3);
    assert_eq!(domain.total(), combines);
    assert_eq!(domain.or.get(), 2);
    assert_eq!(domain.and.get(), 1);
    // Both gates see the shared step's value.
    assert_eq!(eval.get(g1), Some(&9));
    assert_eq!(eval.get(g2), Some(&9));
    assert_eq!(*eval.root_value(), 9);
    Ok(())
}

/// Ladder DAG: the root over two gates per level, each gate over both gates
/// of the next level, and the last level over two shared steps. There are
/// 2^levels root-to-leaf paths.
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

#[test]
fn ladder_is_linear() -> Res {
    let tree = ladder(64)?;
    let steps = tree.iter().filter(|(i, _, _)| is_step(&tree, *i)).count();
    let gates = tree.len() - steps;
    let edges: usize = tree.iter().map(|(i, _, _)| tree.children(i).len()).sum();
    assert_eq!((steps, gates, edges), (2, 129, 258));

    let leaf_calls = Cell::new(0_usize);
    let domain = Counting::new(MaxMin::<u8>::new());
    let eval = evaluate(&tree, &domain, |i| {
        leaf_calls.set(leaf_calls.get() + 1);
        Some(u8::try_from(i.index() % 7).unwrap_or(0))
    })?;
    assert_eq!(leaf_calls.get(), steps);
    assert_eq!(domain.total(), edges - gates);
    assert_eq!(eval.len(), tree.len());
    Ok(())
}

#[test]
fn eval_errors() -> Res {
    const LABEL: &str = "secret customer label";
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, LABEL))?;
    let r = tree.root();
    let a = tree.add_child(r, NodeId::new_v7(), Node::step(LABEL))?;
    let g = tree.add_child(r, NodeId::new_v7(), Node::gate(GateKind::And, LABEL))?;
    let (a_id, g_id) = (id(&tree, a)?, id(&tree, g)?);

    let err = evaluate(&tree, &MinTime, |_| Some(1)).err();
    assert_eq!(err, Some(ModelError::EmptyGate(g_id)));
    let text = err.map(|e| e.to_string()).unwrap_or_default();
    assert!(text.contains(&g_id.to_string()), "{text}");
    assert!(!text.contains(LABEL), "{text}");

    let b = tree.add_child(g, NodeId::new_v7(), Node::step(LABEL))?;
    let err = evaluate(&tree, &MinTime, |i| (i != a).then_some(1)).err();
    assert_eq!(err, Some(ModelError::MissingLeafValue(a_id)));
    let text = err.map(|e| e.to_string()).unwrap_or_default();
    assert!(text.contains(&a_id.to_string()), "{text}");
    assert!(!text.contains(LABEL), "{text}");

    // Repaired tree evaluates.
    let eval = evaluate(&tree, &MinTime, |i| Some(if i == b { 2 } else { 5 }))?;
    assert_eq!(*eval.root_value(), 2);
    Ok(())
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

/// A random DAG of `n` nodes with mixed gate kinds; every gate gets at least
/// one child, and about `links` extra edges go from a gate to a node with a
/// larger index (which can never close a cycle).
fn random_dag(n: usize, links: usize, seed: u64) -> Result<AttackTree, ModelError> {
    const KINDS: [GateKind; 3] = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
    let mut rng = Rng(seed);
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let mut gates = vec![tree.root()];
    let mut childless = vec![tree.root()];
    while tree.len() < n {
        // Give childless gates a child first, so no gate stays empty.
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

/// The fixed trees and DAGs for `matches_naive` and `eval_deterministic`.
fn fixtures() -> Result<Vec<AttackTree>, ModelError> {
    let mut out = vec![
        AttackTree::new(NodeId::new_v7(), Node::step("only"))?,
        three_level()?.tree,
        ladder(6)?,
    ];
    // Shared gate subtree under all three gate kinds.
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

/// Naive recursive evaluation of one node, written from the domain
/// definitions without the crate's fold. Exponential on DAGs; fine for the
/// small fixtures.
fn naive<V: Clone>(
    tree: &AttackTree,
    idx: NodeIdx,
    leaf: &dyn Fn(NodeIdx) -> Option<V>,
    rule: &dyn Fn(GateKind, Vec<V>) -> V,
) -> Option<V> {
    let node = tree.get(idx)?;
    match node.gate_kind() {
        None => leaf(idx),
        Some(kind) => {
            let vals: Option<Vec<V>> = tree
                .children(idx)
                .iter()
                .map(|c| naive(tree, *c, leaf, rule))
                .collect();
            let vals = vals?;
            if vals.is_empty() {
                None
            } else {
                Some(rule(kind, vals))
            }
        }
    }
}

fn maxmin_rule(kind: GateKind, vals: Vec<u8>) -> u8 {
    let it = vals.into_iter();
    match kind {
        GateKind::Or => it.max(),
        GateKind::And | GateKind::SeqAnd => it.min(),
    }
    .unwrap_or(0)
}

fn min_time_rule(kind: GateKind, vals: Vec<u64>) -> u64 {
    let mut it = vals.into_iter();
    match kind {
        GateKind::Or => it.min().unwrap_or(0),
        GateKind::And => it.max().unwrap_or(0),
        GateKind::SeqAnd => it.try_fold(0_u64, u64::checked_add).unwrap_or(u64::MAX),
    }
}

fn assert_matches<V: Clone + PartialEq + std::fmt::Debug>(
    tree: &AttackTree,
    eval: &Evaluation<V>,
    leaf: &dyn Fn(NodeIdx) -> Option<V>,
    rule: &dyn Fn(GateKind, Vec<V>) -> V,
) {
    assert_eq!(eval.len(), tree.len());
    for (i, _, _) in tree.iter() {
        assert_eq!(
            eval.get(i).cloned(),
            naive(tree, i, leaf, rule),
            "node {}",
            i.index()
        );
    }
}

#[test]
fn matches_naive() -> Res {
    for (k, tree) in fixtures()?.iter().enumerate() {
        let shift = u64::try_from(k).unwrap_or(0);
        let level = |i: NodeIdx| Some(u8::try_from((i.index() * 7 + k) % 5).unwrap_or(0));
        let eval = evaluate(tree, &MaxMin::<u8>::new(), level)?;
        assert_matches(tree, &eval, &level, &maxmin_rule);

        // Some huge times so that SAND saturates in places.
        let time = |i: NodeIdx| {
            let i = i.index() as u64;
            Some(if (i + shift) % 11 == 0 {
                u64::MAX / 2
            } else {
                (i * 13 + shift) % 100
            })
        };
        let eval = evaluate(tree, &MinTime, time)?;
        assert_matches(tree, &eval, &time, &min_time_rule);
    }
    Ok(())
}

#[test]
fn eval_deterministic() -> Res {
    for tree in fixtures()? {
        let level = |i: NodeIdx| Some(u8::try_from(i.index() % 9).unwrap_or(0));
        let first = evaluate(&tree, &MaxMin::<u8>::new(), level)?;
        let second = evaluate(&tree, &MaxMin::<u8>::new(), level)?;
        let copy = tree.clone();
        let third = evaluate(&copy, &MaxMin::<u8>::new(), level)?;
        assert_eq!(first, second);
        assert_eq!(first, third);
        assert_eq!(format!("{first:?}"), format!("{third:?}"));
        let a: Vec<_> = first.iter().collect();
        let b: Vec<_> = third.iter().collect();
        assert_eq!(a, b);
    }
    Ok(())
}

#[test]
fn deep_chain_eval() -> Res {
    const KINDS: [GateKind; 3] = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
    const DEPTH: usize = 100_000;
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "g"))?;
    let mut last = tree.root();
    for k in 1..DEPTH {
        last = gate(&mut tree, last, KINDS[k % KINDS.len()])?;
    }
    let s = step(&mut tree, last)?;
    let eval = evaluate(&tree, &MinTime, |i| (i == s).then_some(42))?;
    assert_eq!(*eval.root_value(), 42);
    assert_eq!(eval.len(), DEPTH + 1);
    assert!(eval.iter().all(|(_, v)| *v == 42));
    Ok(())
}

#[test]
fn evaluation_accessors() -> Res {
    let small = three_level()?.tree;
    let large = ladder(8)?;
    assert!(large.len() > small.len());
    let eval = evaluate(&small, &MaxMin::<u8>::new(), |_| Some(1))?;

    let mut outside = 0;
    for (i, _, _) in large.iter() {
        if i.index() >= small.len() {
            assert_eq!(eval.get(i), None);
            outside += 1;
        }
    }
    assert!(outside > 0);
    assert_eq!(Some(eval.root_value()), eval.get(small.root()));
    assert_eq!(eval.len(), small.len());
    assert!(!eval.is_empty());
    let order: Vec<NodeIdx> = eval.iter().map(|(i, _)| i).collect();
    let arena: Vec<NodeIdx> = small.iter().map(|(i, _, _)| i).collect();
    assert_eq!(order, arena);
    Ok(())
}
