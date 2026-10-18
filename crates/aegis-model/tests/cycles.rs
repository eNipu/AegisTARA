//! Links and cycles through the public API: random `link` sequences are
//! accepted exactly when they keep the graph acyclic, and deep chains do not
//! overflow the stack. The arena-level property test for Kahn's algorithm on
//! graphs that do contain cycles is in `src/topo.rs` (cycles cannot be built
//! through the public API).

use std::collections::VecDeque;

use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId, NodeIdx};
use proptest::prelude::*;

/// Reference reachability: breadth-first search from `from`, written
/// independently of the crate's DFS. Time O(V + E).
fn reaches(tree: &AttackTree, from: NodeIdx, to: NodeIdx) -> bool {
    let mut seen = vec![false; tree.len()];
    let mut queue = VecDeque::from([from]);
    seen[from.index()] = true;
    while let Some(n) = queue.pop_front() {
        if n == to {
            return true;
        }
        for &c in tree.children(n) {
            if !seen[c.index()] {
                seen[c.index()] = true;
                queue.push_back(c);
            }
        }
    }
    false
}

fn idx(tree: &AttackTree, id: &NodeId) -> Result<NodeIdx, TestCaseError> {
    tree.idx_of(id)
        .ok_or_else(|| TestCaseError::fail("ID in error is not in the tree"))
}

/// Children and parents of every node.
type Snapshot = Vec<(Vec<NodeIdx>, Vec<NodeIdx>)>;

fn snapshot(tree: &AttackTree) -> Snapshot {
    tree.iter()
        .map(|(i, _, _)| (tree.children(i).to_vec(), tree.parents(i).collect()))
        .collect()
}

/// Checks the topological order and the parent index through the public API.
fn check(tree: &AttackTree) -> Result<(), TestCaseError> {
    let order = tree
        .topological_order()
        .map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(order.len(), tree.len());
    let mut pos = vec![usize::MAX; tree.len()];
    for (k, n) in order.iter().enumerate() {
        prop_assert_eq!(pos[n.index()], usize::MAX, "repeated node");
        pos[n.index()] = k;
    }
    for (p, _, node) in tree.iter() {
        for &c in node.children() {
            prop_assert!(pos[p.index()] < pos[c.index()]);
            prop_assert_eq!(tree.parents(c).filter(|q| *q == p).count(), 1);
        }
        for q in tree.parents(p) {
            prop_assert_eq!(tree.children(q).iter().filter(|c| **c == p).count(), 1);
        }
        prop_assert_eq!(tree.parents(p).next(), tree.owner(p));
    }
    prop_assert_eq!(tree.clone().topological_order(), Ok(order));
    Ok(())
}

fn build(nodes: &[(usize, u8)]) -> Result<AttackTree, ModelError> {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let mut gates = vec![tree.root()];
    for &(p, kind) in nodes {
        let parent = gates[p % gates.len()];
        let node = match kind {
            0 => Node::gate(GateKind::And, "and"),
            1 => Node::gate(GateKind::Or, "or"),
            2 => Node::gate(GateKind::SeqAnd, "seq"),
            _ => Node::step("step"),
        };
        let is_gate = node.gate_kind().is_some();
        let idx = tree.add_child(parent, NodeId::new_v7(), node)?;
        if is_gate {
            gates.push(idx);
        }
    }
    Ok(tree)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Every `link` succeeds exactly when the reference says `child` does
    /// not reach `parent` (and the edge is new and `parent` is a gate). A
    /// rejected link reports a valid path and changes nothing. The graph
    /// stays acyclic throughout.
    #[test]
    fn link_matches_reference(
        nodes in prop::collection::vec((any::<usize>(), 0u8..4), 0..500),
        links in prop::collection::vec((any::<usize>(), any::<usize>()), 0..=500),
    ) {
        let mut tree = build(&nodes).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let all: Vec<NodeIdx> = tree.iter().map(|(i, _, _)| i).collect();
        for (a, b) in links {
            let p = all[a % all.len()];
            let c = all[b % all.len()];
            let is_step = tree.get(p).is_some_and(|n| n.gate_kind().is_none());
            let duplicate = tree.children(p).contains(&c);
            let would_cycle = reaches(&tree, c, p);
            let before = snapshot(&tree);
            match tree.link(p, c) {
                Ok(()) => {
                    prop_assert!(!is_step && !duplicate && !would_cycle);
                    prop_assert_eq!(tree.children(p).last(), Some(&c));
                    prop_assert_eq!(tree.parents(c).last(), Some(p));
                }
                Err(e) => {
                    prop_assert_eq!(snapshot(&tree), before, "failed link changed the tree");
                    match e {
                        ModelError::NotAGate(q) => {
                            prop_assert!(is_step);
                            prop_assert_eq!(q, p);
                        }
                        ModelError::DuplicateEdge { parent, child } => {
                            prop_assert!(!is_step && duplicate);
                            prop_assert_eq!(idx(&tree, &parent)?, p);
                            prop_assert_eq!(idx(&tree, &child)?, c);
                        }
                        ModelError::WouldCreateCycle { parent, child, path } => {
                            prop_assert!(!is_step && !duplicate && would_cycle);
                            prop_assert_eq!(idx(&tree, &parent)?, p);
                            prop_assert_eq!(idx(&tree, &child)?, c);
                            prop_assert_eq!(path.first(), Some(&child));
                            prop_assert_eq!(path.last(), Some(&parent));
                            let path: Vec<NodeIdx> =
                                path.iter().map(|id| idx(&tree, id)).collect::<Result<_, _>>()?;
                            let mut seen = vec![false; tree.len()];
                            for n in &path {
                                prop_assert!(!seen[n.index()], "repeated node in path");
                                seen[n.index()] = true;
                            }
                            for w in path.windows(2) {
                                prop_assert!(tree.children(w[0]).contains(&w[1]));
                            }
                        }
                        other => return Err(TestCaseError::fail(format!("unexpected {other}"))),
                    }
                }
            }
        }
        check(&tree)?;
    }
}

/// `large` reference size from the engineering standards.
const CHAIN: usize = 100_000;

fn chain() -> (AttackTree, Vec<NodeIdx>) {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "g")).expect("root");
    let label: std::sync::Arc<str> = "g".into();
    let mut nodes = Vec::with_capacity(CHAIN);
    nodes.push(tree.root());
    for _ in 1..CHAIN {
        let last = *nodes.last().expect("non-empty");
        let node = Node::Gate {
            kind: GateKind::And,
            label: std::sync::Arc::clone(&label),
            children: Vec::new(),
        };
        nodes.push(tree.add_child(last, NodeId::new_v7(), node).expect("add"));
    }
    (tree, nodes)
}

#[test]
fn deep_chain_orders() {
    let (tree, nodes) = chain();
    assert_eq!(tree.topological_order(), Ok(nodes));
}

#[test]
fn deep_chain_rejects_cycle() {
    let (mut tree, nodes) = chain();
    let root = tree.root();
    let last = *nodes.last().expect("non-empty");
    let ids: Vec<NodeId> = nodes.iter().filter_map(|i| tree.id_of(*i)).collect();
    let want = ModelError::WouldCreateCycle {
        parent: *ids.last().expect("non-empty"),
        child: ids[0],
        path: ids.clone(),
    };
    assert_eq!(tree.link(last, root), Err(want));
    assert_eq!(tree.children(last), []);
    // The shortcut the other way is fine.
    assert_eq!(tree.link(root, last), Ok(()));
    assert_eq!(tree.parents(last).count(), 2);
}
