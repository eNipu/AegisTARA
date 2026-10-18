//! Property tests for `AttackTree`: random `add_child` sequences keep every
//! structural invariant, and failed calls leave the tree unchanged.

use std::collections::HashSet;

use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId, NodeIdx};
use proptest::prelude::*;

/// One random `add_child` call.
#[derive(Clone, Debug)]
struct Op {
    /// Picks the parent among existing nodes (modulo `len`).
    parent: usize,
    /// 0..3 = gate kind, 3 = step.
    kind: u8,
    /// `Some(k)`: reuse the ID of existing node `k % len` (must fail).
    reuse: Option<usize>,
}

fn op() -> impl Strategy<Value = Op> {
    (
        any::<usize>(),
        0u8..4,
        prop::option::weighted(0.1, any::<usize>()),
    )
        .prop_map(|(parent, kind, reuse)| Op {
            parent,
            kind,
            reuse,
        })
}

fn make_node(kind: u8) -> Node {
    match kind {
        0 => Node::gate(GateKind::And, "and"),
        1 => Node::gate(GateKind::Or, "or"),
        2 => Node::gate(GateKind::SeqAnd, "seq"),
        _ => Node::step("step"),
    }
}

/// Checks all invariants through the public API. `inserted` holds the IDs of
/// all successfully added nodes in insertion order. Time O(n).
fn check(tree: &AttackTree, inserted: &[NodeId]) -> Result<(), TestCaseError> {
    prop_assert_eq!(tree.len(), inserted.len());
    let mut seen_children = 0usize;
    let mut count = 0usize;
    for (pos, (idx, id, node)) in tree.iter().enumerate() {
        count += 1;
        // Insertion order.
        prop_assert_eq!(idx.index(), pos);
        prop_assert_eq!(Some(&id), inserted.get(pos));
        // ID map agrees with the arena both ways.
        prop_assert_eq!(tree.idx_of(&id), Some(idx));
        prop_assert_eq!(tree.id_of(idx), Some(id));
        prop_assert_eq!(tree.get(idx), Some(node));
        // Parent links.
        match tree.owner(idx) {
            None => prop_assert_eq!(idx, tree.root()),
            Some(p) => {
                prop_assert_ne!(idx, tree.root());
                prop_assert!(p < idx, "parents are added before children");
                let n = tree.children(p).iter().filter(|c| **c == idx).count();
                prop_assert_eq!(n, 1);
            }
        }
        for c in tree.children(idx) {
            prop_assert_eq!(tree.owner(*c), Some(idx));
        }
        seen_children += tree.children(idx).len();
    }
    prop_assert_eq!(count, tree.len());
    // Every non-root node is a child exactly once overall.
    prop_assert_eq!(seen_children, tree.len() - 1);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn random_add_child_sequences_keep_invariants(ops in prop::collection::vec(op(), 0..=2000)) {
        let root_id = NodeId::new_v7();
        let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "root"))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let mut inserted = vec![root_id];
        let mut ids: HashSet<NodeId> = HashSet::from([root_id]);
        check(&tree, &inserted)?;

        for op in ops {
            let len = tree.len();
            let Some(parent) = tree.idx_of(&inserted[op.parent % len]) else {
                return Err(TestCaseError::fail("parent position out of range"));
            };
            let id = match op.reuse {
                Some(k) => inserted[k % len],
                None => NodeId::new_v7(),
            };
            let parent_is_step = tree.get(parent).is_some_and(|n| n.gate_kind().is_none());
            let before_len = tree.len();
            match tree.add_child(parent, id, make_node(op.kind)) {
                Ok(idx) => {
                    prop_assert!(!parent_is_step);
                    prop_assert!(ids.insert(id), "a duplicate ID was accepted");
                    prop_assert_eq!(idx.index(), before_len);
                    prop_assert_eq!(tree.owner(idx), Some(parent));
                    prop_assert_eq!(tree.children(parent).last(), Some(&idx));
                    inserted.push(id);
                }
                Err(ModelError::NotAGate(p)) => {
                    prop_assert!(parent_is_step);
                    prop_assert_eq!(p, parent);
                    prop_assert_eq!(tree.len(), before_len);
                }
                Err(ModelError::DuplicateId(d)) => {
                    prop_assert!(!parent_is_step);
                    prop_assert_eq!(d, id);
                    prop_assert!(ids.contains(&id));
                    prop_assert_eq!(tree.len(), before_len);
                }
                Err(e) => return Err(TestCaseError::fail(format!("unexpected error {e}"))),
            }
            check(&tree, &inserted)?;
        }
    }

    #[test]
    fn index_from_another_tree_is_rejected(extra in 1usize..50) {
        let mut big = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "big"))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let mut last: NodeIdx = big.root();
        for _ in 0..extra {
            last = big
                .add_child(big.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g"))
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
        let root_id = NodeId::new_v7();
        let mut small = AttackTree::new(root_id, Node::gate(GateKind::Or, "small"))
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(
            small.add_child(last, NodeId::new_v7(), Node::step("x")),
            Err(ModelError::UnknownNode(last))
        );
        check(&small, &[root_id])?;
    }
}
