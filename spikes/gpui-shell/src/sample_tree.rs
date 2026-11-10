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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_tree_has_the_asked_size() -> Result<(), ModelError> {
        assert_eq!(wide_tree(13, 4)?.len(), 13);
        assert_eq!(wide_tree(100_000, 4)?.len(), 100_000);
        Ok(())
    }

    #[test]
    fn wide_tree_labels_gates_by_position() -> Result<(), ModelError> {
        use aegis_canvas::{LayoutConfig, NodeStyle, Scene};
        let tree = wide_tree(3, 4)?;
        let scene = Scene::from_tree(&tree, LayoutConfig::default(), |_| NodeStyle::default());
        assert_eq!(scene.label(0), "Root");
        assert_eq!(scene.label(2), "Gate 2");
        Ok(())
    }
}
