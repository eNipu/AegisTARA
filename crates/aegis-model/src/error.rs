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
    ///
    /// Carries stable public IDs, which stay valid across reloads and in
    /// logs. Errors never contain labels (customer data).
    #[error("node {child} is already a child of node {parent}")]
    DuplicateEdge {
        /// The gate that already has the edge.
        parent: NodeId,
        /// The child that is already linked.
        child: NodeId,
    },
    /// [`AttackTree::link`](crate::AttackTree::link) refused an edge because
    /// `parent` is reachable from `child`, so the new edge would close a
    /// cycle.
    ///
    /// `path` is the existing chain from `child` down to `parent` in edge
    /// order (`path[0] == child`, last element `== parent`); the new edge
    /// `parent -> child` would close it. A self-link gives `path == [child]`.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let root_id = NodeId::new_v7();
    /// let g_id = NodeId::new_v7();
    /// let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "root"))?;
    /// let g = tree.add_child(tree.root(), g_id, Node::gate(GateKind::And, "g"))?;
    /// let err = tree.link(g, tree.root()).err();
    /// assert_eq!(
    ///     err,
    ///     Some(ModelError::WouldCreateCycle {
    ///         parent: g_id,
    ///         child: root_id,
    ///         path: vec![root_id, g_id],
    ///     }),
    /// );
    /// let text = err.map(|e| e.to_string()).unwrap_or_default();
    /// assert_eq!(
    ///     text,
    ///     format!(
    ///         "linking node {root_id} under node {g_id} would create a cycle: \
    ///          {g_id} -> {root_id} -> {g_id}"
    ///     ),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error(
        "linking node {child} under node {parent} would create a cycle: {parent} -> {}",
        path_text(path, false)
    )]
    WouldCreateCycle {
        /// The gate that was to receive the new child.
        parent: NodeId,
        /// The node that was to be linked.
        child: NodeId,
        /// Existing path from `child` down to `parent`, both included.
        path: Vec<NodeId>,
    },
    /// The graph contains a cycle, so it has no topological order.
    ///
    /// [`AttackTree::link`](crate::AttackTree::link) rejects cycles, so this
    /// is a defense-in-depth check before evaluation and is not expected
    /// from a tree built through the public API.
    ///
    /// The path lists the public IDs of one cycle in edge order: `path[i]`
    /// has `path[i + 1]` as a child, and the last node has `path[0]` as a
    /// child. It is non-empty, has no repeats, and starts with the node of
    /// smallest internal index, so the same tree always reports the same
    /// path. Display repeats the first ID at the end.
    ///
    /// ```
    /// use aegis_model::{ModelError, NodeId};
    ///
    /// let (a, b) = (NodeId::new_v7(), NodeId::new_v7());
    /// assert_eq!(
    ///     ModelError::Cycle(vec![a, b]).to_string(),
    ///     format!("cycle in attack tree: {a} -> {b} -> {a}"),
    /// );
    /// ```
    #[error("cycle in attack tree: {}", path_text(.0, true))]
    Cycle(Vec<NodeId>),
}

/// Formats IDs as `a -> b -> c`; with `close`, the first ID is repeated at
/// the end. Time O(path length).
fn path_text(path: &[NodeId], close: bool) -> String {
    let closing = if close { path.first() } else { None };
    let mut out = String::new();
    for n in path.iter().chain(closing) {
        if !out.is_empty() {
            out.push_str(" -> ");
        }
        out.push_str(&n.to_string());
    }
    out
}
