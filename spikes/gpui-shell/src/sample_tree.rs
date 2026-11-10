//! A generated attack tree for the canvas tab.

use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};

/// A balanced tree of `n` nodes (at least 1) with fan-out `k`: an OR root
/// `Root` and AND gates `Gate <i>`, `i` being the tree length before the add.
/// Time O(n).
pub fn wide_tree(n: usize, k: usize) -> Result<AttackTree, ModelError> {
    let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Root"))?;
    let mut frontier = vec![tree.root()];
    let mut next = Vec::new();
    while tree.len() < n && !frontier.is_empty() {
        for parent in &frontier {
            for _ in 0..k {
                if tree.len() >= n {
                    return Ok(tree);
                }
                let label = format!("Gate {}", tree.len());
                let gate = Node::gate(GateKind::And, label);
                next.push(tree.add_child(*parent, NodeId::new_v7(), gate)?);
            }
        }
        frontier = std::mem::take(&mut next);
    }
    Ok(tree)
}
