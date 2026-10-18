//! Domain model for `AegisTARA`.
//!
//! This crate holds an attack tree in an arena: nodes live in one `Vec` and
//! refer to each other by a typed index, [`NodeIdx`]. Every node also has a
//! stable public ID, [`NodeId`] (a `UUIDv7`), that files, APIs, and history use.
//! [`AttackTree`] keeps one map from [`NodeId`] to [`NodeIdx`], so lookups by
//! public ID cost O(1) expected time.
//!
//! A subtree can be shared between gates with [`AttackTree::link`], which
//! turns the tree into a directed graph that may contain cycles.
//! [`AttackTree::topological_order`] orders all nodes parents-first in
//! O(V + E), or reports one cycle as [`ModelError::Cycle`]; call it before
//! evaluating.
//!
//! See `docs/adr/0001-arena-and-typed-ids.md` for why the model is built this
//! way, and `docs/adr/0002-dag-links-and-cycle-detection.md` for links and
//! cycle detection.
//!
//! # Example
//!
//! ```
//! use aegis_model::{AttackTree, GateKind, Node, NodeId};
//!
//! # fn main() -> Result<(), aegis_model::ModelError> {
//! let root_id = NodeId::new_v7();
//! let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "Steal car"))?;
//! let step = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("Relay key fob"))?;
//!
//! assert_eq!(tree.len(), 2);
//! assert_eq!(tree.parent(step), Some(tree.root()));
//! assert_eq!(tree.idx_of(&root_id), Some(tree.root()));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic)]

mod error;
mod id;
mod topo;
mod tree;

pub use error::ModelError;
pub use id::{NodeId, NodeIdx};
pub use tree::{AttackTree, GateKind, Node};
