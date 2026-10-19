//! The three linked-parent layouts from `benches/common` agree with each
//! other and with `AttackTree::parents`, and the shape generator is
//! deterministic and valid (ADR 0004). The benches only mean something if
//! every store answers the same questions the same way.

#![forbid(unsafe_code)]

#[path = "../benches/common/mod.rs"]
mod common;

use std::collections::{BTreeMap, BTreeSet};

use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId, NodeIdx};
use common::{
    BoxStore, Ix, ParentStore, Shape, SparseStore, VecStore, build, shape, shared_target,
};
use proptest::prelude::*;

/// Linked parents of every node, collected through store `S`. Time O(V + L).
fn collect<S: ParentStore>(store: &S, n: usize) -> Vec<Vec<Ix>> {
    (0..n)
        .map(|i| store.linked(Ix::from_usize(i)).collect())
        .collect()
}

/// Reference answer: links grouped per child in list order. Time O(V + L).
fn expected(s: &Shape) -> Vec<Vec<Ix>> {
    let mut out = vec![Vec::new(); s.owners.len()];
    for &(c, p) in &s.links {
        out[c.index()].push(p);
    }
    out
}

/// Checks that all three stores give the reference answer for `s`.
fn assert_agree(s: &Shape) {
    let n = s.owners.len();
    let want = expected(s);
    let v = collect(&build::<VecStore>(n, &s.links), n);
    let b = collect(&build::<BoxStore>(n, &s.links), n);
    let m = collect(&build::<SparseStore>(n, &s.links), n);
    assert_eq!(v, want, "VecStore");
    assert_eq!(b, want, "BoxStore");
    assert_eq!(m, want, "SparseStore");
}

#[test]
fn stores_agree_on_fixed_shapes() {
    for n in [1, 2, 50, 2_000] {
        for share in [0, 1, 10, 50, 100] {
            assert_agree(&shape(n, share, 7));
        }
    }
}

#[test]
fn stores_agree_on_empty_and_out_of_range() {
    let s = shape(0, 50, 1);
    assert!(s.owners.is_empty() && s.links.is_empty());
    let mut v = VecStore::with_nodes(2);
    let mut b = BoxStore::with_nodes(2);
    let mut m = SparseStore::with_nodes(2);
    v.link(Ix(1), Ix(0));
    b.link(Ix(1), Ix(0));
    m.link(Ix(1), Ix(0));
    // Out-of-range children: dense stores ignore the link and report empty.
    v.link(Ix(9), Ix(0));
    b.link(Ix(9), Ix(0));
    assert_eq!(v.linked(Ix(9)).count(), 0);
    assert_eq!(b.linked(Ix(9)).count(), 0);
    assert_eq!(m.linked(Ix(9)).count(), 0);
    assert_eq!(v.linked(Ix(1)).collect::<Vec<_>>(), [Ix(0)]);
    assert_eq!(b.linked(Ix(1)).collect::<Vec<_>>(), [Ix(0)]);
    assert_eq!(m.linked(Ix(1)).collect::<Vec<_>>(), [Ix(0)]);
}

#[test]
fn stores_agree_names_are_distinct() {
    let names = [VecStore::NAME, BoxStore::NAME, SparseStore::NAME];
    assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), 3);
}

#[test]
fn stores_agree_keeps_link_order_not_index_order() {
    let links = [(Ix(3), Ix(2)), (Ix(3), Ix(0)), (Ix(3), Ix(1))];
    for got in [
        collect(&build::<VecStore>(4, &links), 4),
        collect(&build::<BoxStore>(4, &links), 4),
        collect(&build::<SparseStore>(4, &links), 4),
    ] {
        assert_eq!(got[3], [Ix(2), Ix(0), Ix(1)]);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn stores_agree_on_random_shapes(n in 0_usize..=500, share in 0_u32..=100, seed: u64) {
        assert_agree(&shape(n, share, seed));
    }
}

/// Builds an all-gates `AttackTree` from `s.owners` and returns it with the
/// `NodeIdx` of every node (the arena index equals the `Ix`).
fn tree_from(s: &Shape) -> Result<(AttackTree, Vec<NodeIdx>), ModelError> {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "n0"))?;
    let mut idx = vec![tree.root()];
    for (i, owner) in s.owners.iter().enumerate().skip(1) {
        let node = Node::gate(GateKind::And, format!("n{i}"));
        idx.push(tree.add_child(idx[owner.index()], NodeId::new_v7(), node)?);
    }
    Ok((tree, idx))
}

#[test]
fn matches_attack_tree() -> Result<(), ModelError> {
    for seed in [1, 2, 3] {
        let s = shape(2_000, 10, seed);
        let (mut tree, idx) = tree_from(&s)?;
        for (i, ix) in idx.iter().enumerate() {
            assert_eq!(ix.index(), i, "arena index equals Ix");
        }
        for &(c, p) in &s.links {
            tree.link(idx[p.index()], idx[c.index()])?;
        }
        let store = build::<VecStore>(s.owners.len(), &s.links);
        for (i, &node) in idx.iter().enumerate() {
            let mut want: Vec<NodeIdx> = Vec::new();
            if i > 0 {
                want.push(idx[s.owners[i].index()]);
            }
            want.extend(store.linked(Ix::from_usize(i)).map(|p| idx[p.index()]));
            assert_eq!(tree.parents(node).collect::<Vec<_>>(), want, "node {i}");
        }
    }
    Ok(())
}

#[test]
fn shape_is_deterministic() {
    for (n, share) in [(0, 10), (1, 50), (50, 10), (2_000, 50)] {
        assert_eq!(shape(n, share, 42), shape(n, share, 42));
    }
}

#[test]
fn shape_differs_by_seed() {
    let a = shape(2_000, 10, 1);
    let b = shape(2_000, 10, 2);
    assert_ne!(a.links, b.links);
}

/// Checks every validity rule of AC3 on one shape.
fn assert_valid(s: &Shape, share: u32) {
    let n = s.owners.len();
    for (i, o) in s.owners.iter().enumerate().skip(1) {
        assert!(o.index() < i, "owner below child");
    }
    let mut seen = BTreeSet::new();
    let mut per_child: BTreeMap<Ix, usize> = BTreeMap::new();
    for &(c, p) in &s.links {
        assert!(p < c, "parent < child");
        assert_ne!(p, s.owners[c.index()], "parent != owner");
        assert!(seen.insert((c, p)), "no duplicate pair");
        *per_child.entry(c).or_default() += 1;
    }
    for &k in per_child.values() {
        assert!((1..=3).contains(&k), "1 to 3 linked parents");
    }
    let target = shared_target(n, share);
    assert!(
        per_child.len().abs_diff(target) <= 1,
        "shared {} vs target {target} (n {n}, share {share})",
        per_child.len()
    );
}

#[test]
fn shape_is_valid_on_fixed_cases() {
    for n in [0, 1, 2, 3, 50, 2_000, 100_000] {
        for share in [0, 1, 10, 50, 100] {
            assert_valid(&shape(n, share, 9), share);
        }
    }
}

#[test]
fn shape_share_target_rounds() {
    assert_eq!(shared_target(0, 50), 0);
    assert_eq!(shared_target(1, 100), 0);
    assert_eq!(shared_target(100_001, 1), 1_000);
    assert_eq!(shared_target(4, 50), 2); // 1.5 rounds up
    assert_eq!(shared_target(11, 200), 10); // clamped to 100%
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn shape_is_valid_on_random_args(n in 0_usize..=500, share in 0_u32..=100, seed: u64) {
        assert_valid(&shape(n, share, seed), share);
    }
}
