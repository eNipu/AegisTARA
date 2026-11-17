//! Tests of the lifted domain `Applicable<D>`: hand-built cases per gate
//! kind and a property test against a naive recursive evaluator.

use std::collections::HashMap;

use aegis_model::{
    Applicable, AttackTree, Derivation, GateKind, Lifted, MaxMin, Node, NodeId, NodeIdx, evaluate,
};
use proptest::prelude::*;

/// A gate of `kind` over two steps `a` (rating 3) and `b` (rating 5).
fn two_steps(kind: GateKind) -> (AttackTree, NodeIdx, NodeIdx) {
    let mut t = AttackTree::new(NodeId::new_v7(), Node::gate(kind, "root")).expect("tree");
    let a = t
        .add_child(t.root(), NodeId::new_v7(), Node::step("a"))
        .expect("a");
    let b = t
        .add_child(t.root(), NodeId::new_v7(), Node::step("b"))
        .expect("b");
    (t, a, b)
}

/// Evaluates with `Applicable(MaxMin)`; steps in `excluded` are excluded.
fn lifted(t: &AttackTree, a: NodeIdx, excluded: &[NodeIdx]) -> aegis_model::Evaluation<Lifted<u8>> {
    evaluate(t, &Applicable(MaxMin::<u8>::new()), |i| {
        Some(if excluded.contains(&i) {
            Lifted::Excluded
        } else {
            Lifted::Value(if i == a { 3 } else { 5 })
        })
    })
    .expect("eval")
}

const KINDS: [GateKind; 3] = [GateKind::Or, GateKind::And, GateKind::SeqAnd];

#[test]
fn no_child_excluded_matches_plain_max_min() {
    for kind in KINDS {
        let (t, a, b) = two_steps(kind);
        let plain = evaluate(&t, &MaxMin::<u8>::new(), |i| {
            Some(if i == a { 3 } else { 5 })
        })
        .expect("plain");
        let lift = lifted(&t, a, &[]);
        for n in [t.root(), a, b] {
            assert_eq!(
                lift.get(n),
                plain.get(n).map(|v| Lifted::Value(*v)).as_ref()
            );
            assert_eq!(lift.derivation(n), plain.derivation(n), "{kind:?}");
        }
    }
}

#[test]
fn one_child_excluded() {
    for kind in KINDS {
        let (t, a, b) = two_steps(kind);
        let lift = lifted(&t, a, &[b]);
        let root = lift.root_value();
        if kind == GateKind::Or {
            assert_eq!(*root, Lifted::Value(3));
            let sel = Derivation::Selected {
                rule: GateKind::Or,
                child: a,
            };
            assert_eq!(lift.derivation(t.root()), Some(sel));
        } else {
            assert_eq!(*root, Lifted::Excluded, "{kind:?}");
            assert!(root.is_excluded());
            assert_eq!(root.value(), None);
        }
    }
}

#[test]
fn all_children_excluded() {
    for kind in KINDS {
        let (t, a, b) = two_steps(kind);
        let lift = lifted(&t, a, &[a, b]);
        assert_eq!(*lift.root_value(), Lifted::Excluded, "{kind:?}");
    }
}

#[test]
fn or_never_selects_an_excluded_child() {
    let (t, a, b) = two_steps(GateKind::Or);
    // The first child is excluded: the second is selected.
    let lift = lifted(&t, a, &[a]);
    assert_eq!(*lift.root_value(), Lifted::Value(5));
    let sel = |child| Derivation::Selected {
        rule: GateKind::Or,
        child,
    };
    assert_eq!(lift.derivation(t.root()), Some(sel(b)));
    // Three children, the middle one excluded and the strongest otherwise.
    let mut t = t;
    let c = t
        .add_child(t.root(), NodeId::new_v7(), Node::step("c"))
        .expect("c");
    let eval = evaluate(&t, &Applicable(MaxMin::<u8>::new()), |i| {
        Some(match i {
            i if i == b => Lifted::Excluded,
            i if i == c => Lifted::Value(1),
            _ => Lifted::Value(2),
        })
    })
    .expect("eval");
    assert_eq!(*eval.root_value(), Lifted::Value(2));
    assert_eq!(eval.derivation(t.root()), Some(sel(a)));
}

/// One random node: gate kind (or step), parent pick, rating, excluded.
type Spec = (Option<u8>, usize, u8, bool);

fn kind_of(k: u8) -> GateKind {
    KINDS[usize::from(k % 3)]
}

/// Builds a tree from `specs`; every gate gets at least one child. Returns
/// the tree and per-node (rating, excluded) for steps.
fn build(
    root: u8,
    specs: &[Spec],
    links: &[(usize, usize)],
) -> (AttackTree, HashMap<NodeIdx, (u8, bool)>) {
    let mut t = AttackTree::new(NodeId::new_v7(), Node::gate(kind_of(root), "r")).expect("tree");
    let mut gates = vec![t.root()];
    let mut leaves = HashMap::new();
    let add_step = |t: &mut AttackTree, parent, rating, ex| {
        let n = t
            .add_child(parent, NodeId::new_v7(), Node::step("s"))
            .expect("step");
        (n, (rating, ex))
    };
    for &(kind, pick, rating, ex) in specs {
        let parent = gates[pick % gates.len()];
        match kind {
            Some(k) => {
                let g = t
                    .add_child(parent, NodeId::new_v7(), Node::gate(kind_of(k), "g"))
                    .expect("gate");
                gates.push(g);
            }
            None => {
                let (n, v) = add_step(&mut t, parent, rating, ex);
                leaves.insert(n, v);
            }
        }
    }
    for g in gates.clone() {
        if t.children(g).is_empty() {
            let (n, v) = add_step(&mut t, g, 1, false);
            leaves.insert(n, v);
        }
    }
    // Extra edges only from a lower to a higher index: never a cycle.
    let len = t.len();
    for &(from, to) in links {
        let (from, to) = (gates[from % gates.len()], from_index(&t, to % len));
        if to.index() > from.index() {
            let _ = t.link(from, to);
        }
    }
    (t, leaves)
}

fn from_index(t: &AttackTree, i: usize) -> NodeIdx {
    t.iter().nth(i).map(|(n, _, _)| n).expect("index")
}

/// Naive memoized recursion; `None` means excluded.
fn naive(
    t: &AttackTree,
    n: NodeIdx,
    leaves: &HashMap<NodeIdx, (u8, bool)>,
    memo: &mut HashMap<NodeIdx, Option<u8>>,
) -> Option<u8> {
    if let Some(v) = memo.get(&n) {
        return *v;
    }
    let kind = t.get(n).and_then(Node::gate_kind);
    let value = match kind {
        None => leaves.get(&n).and_then(|&(r, ex)| (!ex).then_some(r)),
        Some(kind) => {
            let vals: Vec<_> = t
                .children(n)
                .iter()
                .map(|&c| naive(t, c, leaves, memo))
                .collect();
            if kind == GateKind::Or {
                vals.into_iter().flatten().max()
            } else if vals.iter().any(Option::is_none) {
                None
            } else {
                vals.into_iter().flatten().min()
            }
        }
    };
    memo.insert(n, value);
    value
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        prop::option::weighted(0.4, any::<u8>()),
        any::<usize>(),
        any::<u8>(),
        prop::bool::weighted(0.3),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn matches_a_naive_evaluator(
        root in any::<u8>(),
        specs in prop::collection::vec(spec(), 0..15),
        links in prop::collection::vec((any::<usize>(), any::<usize>()), 0..6),
    ) {
        let (t, leaves) = build(root, &specs, &links);
        prop_assert!(t.len() <= 30);
        let eval = evaluate(&t, &Applicable(MaxMin::<u8>::new()), |i| {
            let &(r, ex) = leaves.get(&i)?;
            Some(if ex { Lifted::Excluded } else { Lifted::Value(r) })
        }).expect("eval");
        let mut memo = HashMap::new();
        for (n, _, node) in t.iter() {
            let want = naive(&t, n, &leaves, &mut memo).map_or(Lifted::Excluded, Lifted::Value);
            prop_assert_eq!(eval.get(n), Some(&want));
            let or_live = node.gate_kind() == Some(GateKind::Or) && !want.is_excluded();
            if let (true, Some(Derivation::Selected { child, .. })) = (or_live, eval.derivation(n)) {
                prop_assert!(!eval.get(child).is_some_and(Lifted::is_excluded));
            }
        }
        let root_want = naive(&t, t.root(), &leaves, &mut memo).map_or(Lifted::Excluded, Lifted::Value);
        prop_assert_eq!(eval.root_value(), &root_want);
    }
}
