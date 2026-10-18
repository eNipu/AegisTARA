//! Cycle detection: property tests against a reference cycle check, and deep
//! chains that would overflow the stack with a recursive algorithm.

use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId, NodeIdx};
use proptest::prelude::*;

/// Reference cycle check: iterative depth-first search with three colors
/// (white, on stack, done). A back edge to an on-stack node means a cycle.
/// Written independently of Kahn's algorithm. Time O(V + E).
fn has_cycle_reference(tree: &AttackTree) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        OnStack,
        Done,
    }
    let all: Vec<NodeIdx> = tree.iter().map(|(i, _, _)| i).collect();
    let mut color = vec![Color::White; tree.len()];
    for &start in &all {
        if color[start.index()] != Color::White {
            continue;
        }
        // Stack of (node, next child position).
        let mut stack = vec![(start, 0usize)];
        color[start.index()] = Color::OnStack;
        while let Some(top) = stack.last_mut() {
            let (n, k) = *top;
            if let Some(&c) = tree.children(n).get(k) {
                top.1 += 1;
                match color[c.index()] {
                    Color::OnStack => return true,
                    Color::White => {
                        color[c.index()] = Color::OnStack;
                        stack.push((c, 0));
                    }
                    Color::Done => {}
                }
            } else {
                color[n.index()] = Color::Done;
                stack.pop();
            }
        }
    }
    false
}

fn is_edge(tree: &AttackTree, p: NodeIdx, c: NodeIdx) -> bool {
    tree.children(p).contains(&c)
}

/// Shape of a random graph: parent choice and kind per added node, then
/// links as (parent position, child position) pairs.
#[derive(Clone, Debug)]
struct Shape {
    nodes: Vec<(usize, u8)>,
    links: Vec<(usize, usize)>,
    /// If set, links only go from a smaller to a larger index, so the graph
    /// is guaranteed acyclic; this keeps both outcomes well covered.
    forward_only: bool,
}

fn shape() -> impl Strategy<Value = Shape> {
    (
        prop::collection::vec((any::<usize>(), 0u8..4), 0..500),
        prop::collection::vec((any::<usize>(), any::<usize>()), 0..=500),
        any::<bool>(),
    )
        .prop_map(|(nodes, links, forward_only)| Shape {
            nodes,
            links,
            forward_only,
        })
}

fn build(shape: &Shape) -> Result<AttackTree, ModelError> {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    let mut gates = vec![tree.root()];
    for &(p, kind) in &shape.nodes {
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
    let all: Vec<NodeIdx> = tree.iter().map(|(i, _, _)| i).collect();
    for &(a, b) in &shape.links {
        let mut p = all[a % all.len()];
        let mut c = all[b % all.len()];
        if shape.forward_only {
            if p == c {
                continue;
            }
            if p > c {
                std::mem::swap(&mut p, &mut c);
            }
        }
        match tree.link(p, c) {
            Ok(()) | Err(ModelError::NotAGate(_) | ModelError::DuplicateEdge { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(tree)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn topo_matches_reference(shape in shape()) {
        let tree = build(&shape).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let expect_cycle = has_cycle_reference(&tree);
        if shape.forward_only {
            prop_assert!(!expect_cycle);
        }
        match tree.topological_order() {
            Ok(order) => {
                prop_assert!(!expect_cycle, "reference found a cycle, Kahn did not");
                prop_assert_eq!(order.len(), tree.len());
                let mut pos = vec![usize::MAX; tree.len()];
                for (k, n) in order.iter().enumerate() {
                    prop_assert!(n.index() < tree.len());
                    prop_assert_eq!(pos[n.index()], usize::MAX, "repeated node");
                    pos[n.index()] = k;
                }
                for (p, _, node) in tree.iter() {
                    for c in node.children() {
                        prop_assert!(pos[p.index()] < pos[c.index()]);
                    }
                }
            }
            Err(ModelError::Cycle(path)) => {
                prop_assert!(expect_cycle, "Kahn found a cycle, reference did not");
                prop_assert!(!path.is_empty());
                let mut seen = vec![false; tree.len()];
                for n in &path {
                    prop_assert!(!seen[n.index()], "repeated node in cycle");
                    seen[n.index()] = true;
                }
                for w in path.windows(2) {
                    prop_assert!(is_edge(&tree, w[0], w[1]));
                }
                let (first, last) = (path[0], path[path.len() - 1]);
                prop_assert!(is_edge(&tree, last, first), "cycle does not close");
                prop_assert_eq!(path.iter().min(), Some(&first));
            }
            Err(e) => return Err(TestCaseError::fail(format!("unexpected error {e}"))),
        }
        // Deterministic on a clone.
        prop_assert_eq!(tree.clone().topological_order(), tree.topological_order());
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
fn deep_chain_cycle() {
    let (mut tree, nodes) = chain();
    let last = *nodes.last().expect("non-empty");
    tree.link(last, tree.root()).expect("link");
    assert_eq!(tree.topological_order(), Err(ModelError::Cycle(nodes)));
}
