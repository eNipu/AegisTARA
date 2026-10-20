//! Domain model for `AegisTARA`.
//!
//! This crate holds an attack tree in an arena: nodes live in one `Vec` and
//! refer to each other by a typed index, [`NodeIdx`]. Every node also has a
//! stable public ID, [`NodeId`] (a `UUIDv7`), that files, APIs, and history use.
//! [`AttackTree`] keeps one map from [`NodeId`] to [`NodeIdx`], so lookups by
//! public ID cost O(1) expected time.
//!
//! A subtree can be shared between gates with [`AttackTree::link`], which
//! turns the tree into a directed acyclic graph (DAG). `link` rejects an edge
//! that would close a cycle ([`ModelError::WouldCreateCycle`]), so the model
//! is always a DAG. [`AttackTree::topological_order`] orders all nodes
//! parents-first in O(V + E) for evaluation, and re-checks for cycles as
//! defense in depth. A node has one [`owner`](AttackTree::owner) and may have
//! more [`parents`](AttackTree::parents). Errors name nodes by their stable
//! [`NodeId`], never by label.
//!
//! [`evaluate`] computes a value for every node in one O(V + E) pass over
//! that order, in an [`AttributeDomain`] chosen by the caller (how OR, AND,
//! and sequential AND combine child values), with the leaf values supplied
//! by the caller. Each node is evaluated once, also when it is shared. The
//! result, an [`Evaluation`], is derived data kept apart from the tree.
//! [`MaxMin`] (feasibility levels) and [`MinTime`] are built in.
//!
//! Every value explains itself: the evaluation records a [`Derivation`] per
//! node (a caller-supplied leaf, one [`Selected`](Derivation::Selected)
//! deciding child, or several children [`Combined`](Derivation::Combined)).
//! [`Evaluation::explain`] returns a node's rule, inputs, and deciding
//! child, and [`Evaluation::why_path`] follows the deciding children from
//! the root down to the step that dominates the result.
//!
//! See `docs/adr/0001-arena-and-typed-ids.md` for why the model is built this
//! way, `docs/adr/0002-dag-links-and-cycle-detection.md` for links and
//! cycle detection, `docs/adr/0005-tree-evaluation.md` for evaluation, and
//! `docs/adr/0006-explain-why-traces.md` for explain-why traces.
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
//! assert_eq!(tree.owner(step), Some(tree.root()));
//! assert_eq!(tree.idx_of(&root_id), Some(tree.root()));
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic)]

mod error;
mod eval;
mod id;
mod topo;
mod tree;

pub use error::ModelError;
pub use eval::{
    AttributeDomain, Derivation, Evaluation, Explanation, Kept, MaxMin, MinTime, evaluate,
};
pub use id::{NodeId, NodeIdx};
pub use tree::{AttackTree, GateKind, Node};
