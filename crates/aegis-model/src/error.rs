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
}
