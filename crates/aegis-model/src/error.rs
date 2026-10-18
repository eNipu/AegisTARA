//! Typed errors for the model crate.

use crate::id::{NodeId, NodeIdx};

/// Errors returned by the model.
///
/// Every operation that returns a `ModelError` leaves the model unchanged.
///
/// ```
/// use aegis_model::{ModelError, NodeId};
///
/// assert_eq!("not a uuid".parse::<NodeId>(), Err(ModelError::InvalidUuid));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ModelError {
    /// The UUID is well formed but is not an RFC 9562 version 7 UUID.
    #[error("UUID is not version 7")]
    NotVersion7,
    /// The text is not a valid UUID.
    #[error("text is not a valid UUID")]
    InvalidUuid,
    /// A node with this public ID already exists in the tree.
    #[error("node ID {0} already exists in the tree")]
    DuplicateId(NodeId),
    /// The index does not refer to a node in this tree.
    #[error("node index {} is not in the tree", .0.index())]
    UnknownNode(NodeIdx),
    /// The node is a step, so it cannot have children.
    #[error("node index {} is a step and cannot have children", .0.index())]
    NotAGate(NodeIdx),
    /// A new node was passed with children; new nodes must start empty.
    #[error("new node must not have children")]
    ChildrenNotEmpty,
    /// The tree cannot hold more nodes than a `u32` index can address.
    #[error("tree node capacity exceeded")]
    CapacityExceeded,
    /// The gate already has this node as a child; a gate holds each child at
    /// most once.
    #[error("node index {} is already a child of node index {}", .child.index(), .parent.index())]
    DuplicateEdge {
        /// The gate that already has the edge.
        parent: NodeIdx,
        /// The child that is already linked.
        child: NodeIdx,
    },
    /// The tree contains a cycle, so it has no topological order.
    ///
    /// The path lists the nodes of one cycle in edge order: `path[i]` has
    /// `path[i + 1]` as a child, and the last node has `path[0]` as a child.
    /// It is non-empty, has no repeats, and starts with its smallest index.
    /// Use [`AttackTree::id_of`](crate::AttackTree::id_of) to turn the
    /// indices into public IDs.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let g = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "g"))?;
    /// tree.link(g, tree.root())?;
    /// let err = tree.topological_order().err();
    /// assert_eq!(err, Some(ModelError::Cycle(vec![tree.root(), g])));
    /// assert_eq!(
    ///     err.map(|e| e.to_string()).as_deref(),
    ///     Some("cycle in attack tree: 0 -> 1 -> 0"),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error("cycle in attack tree: {}", cycle_text(.0))]
    Cycle(Vec<NodeIdx>),
}

/// Formats a cycle path as `i0 -> i1 -> ... -> i0`. Time O(path length).
fn cycle_text(path: &[NodeIdx]) -> String {
    let mut out = String::new();
    for n in path.iter().chain(path.first()) {
        if !out.is_empty() {
            out.push_str(" -> ");
        }
        out.push_str(&n.index().to_string());
    }
    out
}
