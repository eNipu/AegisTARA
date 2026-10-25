//! Tree builders shared by the canvas tests and benchmarks.
#![allow(dead_code)]

use aegis_model::{AttackTree, GateKind, Node, NodeId, NodeIdx};

/// A typed node index by position. The model hands out `NodeIdx` values only
/// from its trees, and a `NodeIdx` is just an arena position, so the first
/// positions of a throwaway tree serve for any tree of the same size.
pub fn idx(i: usize) -> NodeIdx {
    static IDS: std::sync::OnceLock<Vec<NodeIdx>> = std::sync::OnceLock::new();
    IDS.get_or_init(|| wide(5000, 4).iter().map(|(i, _, _)| i).collect())[i]
}

/// A tree from a compact spec. Entry `i` adds node `i + 1`: its parent is
/// the `pick % gates.len()`-th gate so far, and it is a gate when `is_gate`.
/// `links` then try to share nodes between gates; cycles are skipped.
pub fn build(spec: &[(u32, bool)], links: &[(u32, u32)]) -> AttackTree {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Root goal")).expect("root");
    let mut gates: Vec<NodeIdx> = vec![tree.root()];
    for (i, (pick, is_gate)) in spec.iter().enumerate() {
        let parent = gates[*pick as usize % gates.len()];
        let node = if *is_gate {
            let kind = [GateKind::And, GateKind::Or, GateKind::SeqAnd][i % 3];
            Node::gate(kind, format!("Gate {}", i + 1))
        } else {
            Node::step(format!("Step number {} of the attack", i + 1))
        };
        let child = tree
            .add_child(parent, NodeId::new_v7(), node)
            .expect("child");
        if *is_gate {
            gates.push(child);
        }
    }
    let n = tree.len() as u32;
    for (a, b) in links {
        let parent = gates[*a as usize % gates.len()];
        if let Some(child) = Some(idx((*b % n) as usize)) {
            // Cycles and duplicates are refused by the model; skip those.
            let _ = tree.link(parent, child);
        }
    }
    tree
}

/// A balanced-ish tree of about `n` nodes with fan-out `k`, built fast.
pub fn wide(n: usize, k: usize) -> AttackTree {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Root")).expect("root");
    let mut frontier = vec![tree.root()];
    let mut next = Vec::new();
    'grow: loop {
        for parent in &frontier {
            for _ in 0..k {
                if tree.len() >= n {
                    break 'grow;
                }
                let child = tree
                    .add_child(*parent, NodeId::new_v7(), Node::gate(GateKind::And, "Gate"))
                    .expect("child");
                next.push(child);
            }
        }
        frontier = std::mem::take(&mut next);
    }
    tree
}

/// The 12-node tree used for the snapshots: two gates, a shared step.
pub fn sample() -> AttackTree {
    let mut tree =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Steal vehicle")).expect("root");
    let root = tree.root();
    let keyless = tree
        .add_child(
            root,
            NodeId::new_v7(),
            Node::gate(GateKind::SeqAnd, "Keyless entry relay attack"),
        )
        .expect("child");
    let obd = tree
        .add_child(
            root,
            NodeId::new_v7(),
            Node::gate(GateKind::And, "Reprogram key via OBD-II port"),
        )
        .expect("child");
    let _ = tree
        .add_child(keyless, NodeId::new_v7(), Node::step("Amplify fob signal"))
        .expect("child");
    let relay = tree
        .add_child(
            keyless,
            NodeId::new_v7(),
            Node::step("Relay to vehicle <antenna> & start"),
        )
        .expect("child");
    let access = tree
        .add_child(obd, NodeId::new_v7(), Node::step("Gain access to cabin"))
        .expect("child");
    let _ = tree
        .add_child(
            obd,
            NodeId::new_v7(),
            Node::step("Extract immobilizer secret over diagnostic session"),
        )
        .expect("child");
    let _ = tree
        .add_child(obd, NodeId::new_v7(), Node::step("Flash blank key"))
        .expect("child");
    // The cabin step is also needed by the relay chain: a shared node.
    tree.link(keyless, access).expect("link");
    let _ = relay;
    tree
}
