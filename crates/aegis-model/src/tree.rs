//! Arena-based attack tree.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ModelError;
use crate::id::{NodeId, NodeIdx};

/// Most nodes one attack tree in a [`Project`](crate::Project) may hold.
///
/// Bounds the size of one [`Op::AddAttackTree`](crate::Op::AddAttackTree)
/// record and one saved tree, so the save/load decoder can cap every node,
/// edge, and step-feasibility list at decode time and still accept every
/// project the model accepts (see
/// `docs/adr/0014-store-trees-and-provenance.md`). Standalone [`AttackTree`]
/// values outside a [`Project`](crate::Project) may still grow up to
/// `u32::MAX` nodes.
///
/// ```
/// assert_eq!(aegis_model::MAX_ATTACK_TREE_NODES, 4_096);
/// ```
pub const MAX_ATTACK_TREE_NODES: usize = 4_096;

/// Most nodes in a tree that one op may carry whole (`AddAttackTree`, and the
/// inverse of `RemoveAttackTree`): the old per-tree cap. It keeps those ops
/// inside the op log's record cap; a bigger tree is built and taken apart one
/// node at a time (job 065).
///
/// ```
/// assert_eq!(aegis_model::MAX_TREE_OP_NODES, 32);
/// ```
pub const MAX_TREE_OP_NODES: usize = 32;

/// A node taken out of a tree by [`AttackTree::remove_leaf`], with where it
/// was, so that it can be put back exactly (undo).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemovedNode {
    /// The node's public ID.
    pub id: NodeId,
    /// The node.
    pub node: Node,
    /// Its arena index before the removal.
    pub index: NodeIdx,
    /// Its owner (before the removal; the owner is before the node, so its
    /// index did not change).
    pub owner: NodeIdx,
    /// Its position among the owner's children.
    pub position: usize,
}

/// Decoded parts of one attack tree node: `(id, label, gate_and_children, parents)`.
pub(crate) type WireNodeParts = (
    NodeId,
    Arc<str>,
    Option<(GateKind, Vec<NodeId>)>,
    Option<Vec<NodeId>>,
);

/// How a gate combines its children.
///
/// ```
/// use aegis_model::GateKind;
///
/// assert_ne!(GateKind::And, GateKind::SeqAnd);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GateKind {
    /// All children are needed, in any order.
    And,
    /// Any one child is enough.
    Or,
    /// All children are needed, in the listed order.
    SeqAnd,
}

/// One node of an attack tree: a gate or a leaf step.
///
/// Using an enum makes invalid states unrepresentable: a step has no
/// children field at all.
///
/// ```
/// use aegis_model::{GateKind, Node};
///
/// let gate = Node::gate(GateKind::Or, "Open door");
/// let step = Node::step("Pick lock");
/// assert_eq!(gate.label(), "Open door");
/// assert!(step.children().is_empty());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A gate that combines its children.
    Gate {
        /// How the children are combined.
        kind: GateKind,
        /// Display label, shared to avoid copies.
        label: Arc<str>,
        /// Child indices in display order. Must be empty when the node is
        /// passed to [`AttackTree::new`] or [`AttackTree::add_child`].
        children: Vec<NodeIdx>,
    },
    /// A leaf attack step.
    Step {
        /// Display label, shared to avoid copies.
        label: Arc<str>,
    },
}

impl Node {
    /// Creates a gate with no children.
    ///
    /// ```
    /// use aegis_model::{GateKind, Node};
    ///
    /// let n = Node::gate(GateKind::SeqAnd, "Chain");
    /// assert_eq!(n.gate_kind(), Some(GateKind::SeqAnd));
    /// ```
    pub fn gate(kind: GateKind, label: impl Into<Arc<str>>) -> Self {
        Node::Gate {
            kind,
            label: label.into(),
            children: Vec::new(),
        }
    }

    /// Creates a leaf step.
    ///
    /// ```
    /// use aegis_model::Node;
    ///
    /// let n = Node::step("Sniff CAN bus");
    /// assert_eq!(n.gate_kind(), None);
    /// ```
    pub fn step(label: impl Into<Arc<str>>) -> Self {
        Node::Step {
            label: label.into(),
        }
    }

    /// Returns the label.
    ///
    /// ```
    /// use aegis_model::Node;
    ///
    /// assert_eq!(Node::step("x").label(), "x");
    /// ```
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Node::Gate { label, .. } | Node::Step { label } => label,
        }
    }

    /// Returns the gate kind, or `None` for a step.
    ///
    /// ```
    /// use aegis_model::{GateKind, Node};
    ///
    /// assert_eq!(Node::gate(GateKind::And, "g").gate_kind(), Some(GateKind::And));
    /// assert_eq!(Node::step("s").gate_kind(), None);
    /// ```
    #[must_use]
    pub fn gate_kind(&self) -> Option<GateKind> {
        match self {
            Node::Gate { kind, .. } => Some(*kind),
            Node::Step { .. } => None,
        }
    }

    /// Returns the children of a gate, or an empty slice for a step.
    ///
    /// ```
    /// use aegis_model::Node;
    ///
    /// assert!(Node::step("s").children().is_empty());
    /// ```
    #[must_use]
    pub fn children(&self) -> &[NodeIdx] {
        match self {
            Node::Gate { children, .. } => children,
            Node::Step { .. } => &[],
        }
    }
}

/// An attack tree stored in an arena.
///
/// Nodes live in a `Vec` indexed by [`NodeIdx`]. Public IDs ([`NodeId`]) and
/// parent links are kept in parallel arrays with the same index. One
/// `HashMap` (`SipHash`, because IDs come from untrusted files and network
/// input) resolves a [`NodeId`] to its [`NodeIdx`].
///
/// Nodes are added with [`add_child`](Self::add_child), each one attached to
/// its *owner* gate. [`link`](Self::link) can then add an existing node as an
/// extra child of another gate, to share a subtree. The structure is a
/// directed acyclic graph (DAG): `link` rejects any edge that would close a
/// cycle. [`topological_order`](Self::topological_order) gives the
/// evaluation order and re-checks for cycles as defense in depth.
///
/// Invariants, kept by every method:
/// - `nodes`, `ids`, `owners`, `linked_parents`, and `by_id` all have the
///   same length;
/// - `by_id[ids[i]] == i` for every index `i`;
/// - the root has no owner; every other node has exactly one owner and
///   appears exactly once in its owner's children;
/// - a node may also appear in other gates' children through `link`, at most
///   once per gate; every child index refers to a node in this tree;
/// - for every edge `p -> c`, `p` appears exactly once in
///   [`parents(c)`](Self::parents), and every gate in `parents(c)` has `c`
///   as a child exactly once;
/// - there is no cycle.
///
/// ```
/// use aegis_model::{AttackTree, GateKind, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
/// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
/// assert_eq!(tree.children(tree.root()), &[a, b]);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttackTree {
    /// Arena; position = `NodeIdx`.
    nodes: Vec<Node>,
    /// Index to public ID (parallel to `nodes`).
    ids: Vec<NodeId>,
    /// Index to owner gate (parallel to `nodes`); `None` only for the root.
    owners: Vec<Option<NodeIdx>>,
    /// Index to the extra parents added by `link`, in link order (parallel
    /// to `nodes`). An empty `Vec` does not allocate, so plain tree nodes
    /// cost only the 24-byte header.
    linked_parents: Vec<Vec<NodeIdx>>,
    /// Public ID to index. Never iterated for output; order comes from the
    /// arena.
    by_id: HashMap<NodeId, NodeIdx>,
    /// Index of the root node.
    root: NodeIdx,
    /// Number of extra edges added by [`link`](Self::link).
    link_count: u32,
}

impl AttackTree {
    /// Creates a tree with a single root node.
    ///
    /// Time O(1), space O(1).
    ///
    /// # Errors
    ///
    /// [`ModelError::ChildrenNotEmpty`] if `root` is a gate with children.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// assert_eq!(tree.len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(root_id: NodeId, root: Node) -> Result<Self, ModelError> {
        if !root.children().is_empty() {
            return Err(ModelError::ChildrenNotEmpty);
        }
        let root_idx = NodeIdx::try_from_usize(0).ok_or(ModelError::CapacityExceeded)?;
        let mut by_id = HashMap::new();
        by_id.insert(root_id, root_idx);
        Ok(AttackTree {
            nodes: vec![root],
            ids: vec![root_id],
            owners: vec![None],
            linked_parents: vec![Vec::new()],
            by_id,
            root: root_idx,
            link_count: 0,
        })
    }

    /// Adds `node` as the last child of `parent` and returns its index.
    ///
    /// Time O(1) amortized (one `Vec` push per array, one expected-O(1) map
    /// insert), space O(1) amortized.
    ///
    /// # Errors
    ///
    /// Checked in this order; on any error the tree is unchanged:
    /// - [`ModelError::UnknownNode`] if `parent` is not in this tree;
    /// - [`ModelError::NotAGate`] if `parent` is a step;
    /// - [`ModelError::ChildrenNotEmpty`] if `node` is a gate with children;
    /// - [`ModelError::DuplicateId`] if `id` is already used in this tree;
    /// - [`ModelError::CapacityExceeded`] if the arena is full.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let id = NodeId::new_v7();
    /// let step = tree.add_child(tree.root(), id, Node::step("s"))?;
    /// assert_eq!(tree.owner(step), Some(tree.root()));
    /// assert_eq!(
    ///     tree.add_child(step, NodeId::new_v7(), Node::step("t")),
    ///     Err(ModelError::NotAGate(step)),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_child(
        &mut self,
        parent: NodeIdx,
        id: NodeId,
        node: Node,
    ) -> Result<NodeIdx, ModelError> {
        match self.nodes.get(parent.index()) {
            None => return Err(ModelError::UnknownNode(parent)),
            Some(Node::Step { .. }) => return Err(ModelError::NotAGate(parent)),
            Some(Node::Gate { .. }) => {}
        }
        if !node.children().is_empty() {
            return Err(ModelError::ChildrenNotEmpty);
        }
        if self.by_id.contains_key(&id) {
            return Err(ModelError::DuplicateId(id));
        }
        let idx = NodeIdx::try_from_usize(self.nodes.len()).ok_or(ModelError::CapacityExceeded)?;

        // All checks passed; from here on nothing can fail, so the tree is
        // never left half-updated.
        if let Some(Node::Gate { children, .. }) = self.nodes.get_mut(parent.index()) {
            children.push(idx);
        }
        self.nodes.push(node);
        self.ids.push(id);
        self.owners.push(Some(parent));
        self.linked_parents.push(Vec::new());
        self.by_id.insert(id, idx);
        Ok(idx)
    }

    /// Adds the existing node `child` as the last child of the gate `parent`,
    /// so the two gates share it (the tree becomes a DAG).
    ///
    /// The owner of `child` ([`owner`](Self::owner)) does not change;
    /// `parent` is appended to [`parents(child)`](Self::parents). The edge is
    /// rejected if it would close a cycle, that is, if `parent` is reachable
    /// from `child` (this includes a self-link and linking the root, which
    /// reaches every node). See `docs/adr/0002-dag-links-and-cycle-detection.md`.
    ///
    /// Time O(k + d + e): k = children of `parent` (duplicate scan), d and e =
    /// descendants of `child` and their edges (iterative DFS), plus one zeroed
    /// O(V) allocation for the visited flags; at most O(V + E). Space O(V).
    ///
    /// # Errors
    ///
    /// Checked in this order; on any error the tree is unchanged:
    /// - [`ModelError::UnknownNode`] if `parent` is not in this tree;
    /// - [`ModelError::UnknownNode`] if `child` is not in this tree;
    /// - [`ModelError::NotAGate`] if `parent` is a step;
    /// - [`ModelError::DuplicateEdge`] if `child` is already a child of
    ///   `parent`;
    /// - [`ModelError::WouldCreateCycle`] if `parent` is reachable from
    ///   `child`, with the existing path from `child` down to `parent`.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// let g1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g1"))?;
    /// let g2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g2"))?;
    /// let fob_id = NodeId::new_v7();
    /// let fob = tree.add_child(g1, fob_id, Node::step("obtain key fob"))?;
    /// tree.link(g2, fob)?;
    /// assert_eq!(tree.children(g2), &[fob]);
    /// assert_eq!(tree.owner(fob), Some(g1));
    /// assert_eq!(tree.parents(fob).collect::<Vec<_>>(), [g1, g2]);
    /// let g2_id = tree.id_of(g2).ok_or(ModelError::UnknownNode(g2))?;
    /// assert_eq!(
    ///     tree.link(g2, fob),
    ///     Err(ModelError::DuplicateEdge { parent: g2_id, child: fob_id }),
    /// );
    /// assert!(matches!(
    ///     tree.link(g1, tree.root()),
    ///     Err(ModelError::WouldCreateCycle { .. }),
    /// ));
    /// # Ok(())
    /// # }
    /// ```
    pub fn link(&mut self, parent: NodeIdx, child: NodeIdx) -> Result<(), ModelError> {
        let position = self
            .nodes
            .get(parent.index())
            .map_or(0, |n| n.children().len());
        let slot = self.linked_parents.get(child.index()).map_or(0, Vec::len);
        self.link_at(parent, child, position, slot)
    }

    /// Like [`link`](Self::link), at a chosen place: `position` among the
    /// children of `parent`, and `slot` among the extra parents of `child`.
    /// Undoing an [`unlink`](Self::unlink) uses it to restore both orders.
    ///
    /// Time as for `link`, plus O(k + p) to insert.
    ///
    /// # Errors
    ///
    /// As [`link`](Self::link), then [`ModelError::BadNodePlacement`] if
    /// `position` or `slot` is past the end.
    pub fn link_at(
        &mut self,
        parent: NodeIdx,
        child: NodeIdx,
        position: usize,
        slot: usize,
    ) -> Result<(), ModelError> {
        self.check_link(parent, child, position, slot)?;
        // The checks passed, so both slots exist and both inserts happen.
        if let (Some(Node::Gate { children, .. }), Some(linked)) = (
            self.nodes.get_mut(parent.index()),
            self.linked_parents.get_mut(child.index()),
        ) {
            children.insert(position, child);
            linked.insert(slot, parent);
            self.link_count = self.link_count.saturating_add(1);
        }
        Ok(())
    }

    /// Everything `link_at` checks, without changing the tree.
    pub(crate) fn check_link(
        &self,
        parent: NodeIdx,
        child: NodeIdx,
        position: usize,
        slot: usize,
    ) -> Result<(), ModelError> {
        let (Some(parent_node), Some(&parent_id)) =
            (self.nodes.get(parent.index()), self.ids.get(parent.index()))
        else {
            return Err(ModelError::UnknownNode(parent));
        };
        let Some(&child_id) = self.ids.get(child.index()) else {
            return Err(ModelError::UnknownNode(child));
        };
        let Node::Gate { .. } = parent_node else {
            return Err(ModelError::NotAGate(parent));
        };
        if parent_node.children().contains(&child) {
            return Err(ModelError::DuplicateEdge {
                parent: parent_id,
                child: child_id,
            });
        }
        if let Some(path) = crate::topo::find_path(&self.nodes, child, parent) {
            return Err(ModelError::WouldCreateCycle {
                parent: parent_id,
                child: child_id,
                path: self.ids_for(&path),
            });
        }
        let slots = self.linked_parents.get(child.index()).map_or(0, Vec::len);
        if position > parent_node.children().len() || slot > slots {
            return Err(ModelError::BadNodePlacement);
        }
        Ok(())
    }

    /// Removes the extra edge `parent -> child` made by [`link`](Self::link)
    /// and returns where it was: its position among the children of `parent`
    /// and its slot among the extra parents of `child`. The owner's edge is
    /// not an extra edge and cannot be removed this way.
    ///
    /// Time O(k + p).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownNode`] for a node not in this tree,
    /// [`ModelError::NotLinked`] if there is no such extra edge.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// let g1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g1"))?;
    /// let g2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g2"))?;
    /// let s = tree.add_child(g1, NodeId::new_v7(), Node::step("s"))?;
    /// tree.link(g2, s)?;
    /// assert_eq!(tree.unlink(g2, s)?, (0, 0));
    /// assert!(matches!(tree.unlink(g1, s), Err(ModelError::NotLinked { .. })));
    /// # Ok(())
    /// # }
    /// ```
    pub fn unlink(
        &mut self,
        parent: NodeIdx,
        child: NodeIdx,
    ) -> Result<(usize, usize), ModelError> {
        let (position, slot) = self.check_unlink(parent, child)?;
        if let Some(Node::Gate { children, .. }) = self.nodes.get_mut(parent.index()) {
            children.remove(position);
        }
        if let Some(linked) = self.linked_parents.get_mut(child.index()) {
            linked.remove(slot);
        }
        self.link_count = self.link_count.saturating_sub(1);
        Ok((position, slot))
    }

    /// Everything `unlink` checks, and where the edge is: `(position, slot)`.
    pub(crate) fn check_unlink(
        &self,
        parent: NodeIdx,
        child: NodeIdx,
    ) -> Result<(usize, usize), ModelError> {
        let (Some(&parent_id), Some(&child_id)) =
            (self.ids.get(parent.index()), self.ids.get(child.index()))
        else {
            return Err(ModelError::UnknownNode(
                if self.ids.get(parent.index()).is_none() {
                    parent
                } else {
                    child
                },
            ));
        };
        let not_linked = ModelError::NotLinked {
            parent: parent_id,
            child: child_id,
        };
        let slot = self
            .linked_parents
            .get(child.index())
            .and_then(|l| l.iter().position(|p| *p == parent))
            .ok_or_else(|| not_linked.clone())?;
        let Some(Node::Gate { children, .. }) = self.nodes.get(parent.index()) else {
            return Err(not_linked);
        };
        let position = children
            .iter()
            .position(|c| *c == child)
            .ok_or(not_linked)?;
        Ok((position, slot))
    }

    /// Inserts `node` as a child of the gate `parent`, at arena index `at`
    /// and at `position` among its children, and shifts every later node up
    /// by one. [`add_child`](Self::add_child) is the case `at == len()` and
    /// `position == children(parent).len()`. A new owner must come before its
    /// node in the arena, so `at` is after `parent` and at most `len()`.
    ///
    /// Time O(V + E) for the shift (one pass over the arena).
    ///
    /// # Errors
    ///
    /// On any error the tree is unchanged: [`ModelError::UnknownNode`],
    /// [`ModelError::NotAGate`], [`ModelError::ChildrenNotEmpty`],
    /// [`ModelError::DuplicateId`], [`ModelError::BadNodePlacement`] for an
    /// `at` or `position` out of range, [`ModelError::CapacityExceeded`].
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let b_id = NodeId::new_v7();
    /// let b = tree.add_child(tree.root(), b_id, Node::step("b"))?;
    /// // Insert "a" at b's index, first among the children: "b" moves to index 2.
    /// let a = tree.insert_child(tree.root(), b, 0, NodeId::new_v7(), Node::step("a"))?;
    /// let b = tree.idx_of(&b_id).ok_or(ModelError::UnknownNode(b))?;
    /// assert_eq!((a.index(), b.index()), (1, 2));
    /// assert_eq!(tree.children(tree.root()), &[a, b]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn insert_child(
        &mut self,
        parent: NodeIdx,
        at: NodeIdx,
        position: usize,
        id: NodeId,
        node: Node,
    ) -> Result<NodeIdx, ModelError> {
        self.check_insert(parent, at, position, id, &node)?;
        let from = at.index();
        self.shift(from, 1);
        self.nodes.insert(from, node);
        self.ids.insert(from, id);
        self.owners.insert(from, Some(parent));
        self.linked_parents.insert(from, Vec::new());
        self.by_id.insert(id, at);
        if let Some(Node::Gate { children, .. }) = self.nodes.get_mut(parent.index()) {
            children.insert(position, at);
        }
        Ok(at)
    }

    /// Everything `insert_child` checks, without changing the tree.
    pub(crate) fn check_insert(
        &self,
        parent: NodeIdx,
        at: NodeIdx,
        position: usize,
        id: NodeId,
        node: &Node,
    ) -> Result<(), ModelError> {
        match self.nodes.get(parent.index()) {
            None => return Err(ModelError::UnknownNode(parent)),
            Some(Node::Step { .. }) => return Err(ModelError::NotAGate(parent)),
            Some(Node::Gate { children, .. }) => {
                if position > children.len() {
                    return Err(ModelError::BadNodePlacement);
                }
            }
        }
        if !node.children().is_empty() {
            return Err(ModelError::ChildrenNotEmpty);
        }
        if self.by_id.contains_key(&id) {
            return Err(ModelError::DuplicateId(id));
        }
        if at.index() <= parent.index() || at.index() > self.nodes.len() {
            return Err(ModelError::BadNodePlacement);
        }
        NodeIdx::try_from_usize(self.nodes.len()).ok_or(ModelError::CapacityExceeded)?;
        Ok(())
    }

    /// Removes the node `idx` and shifts every later node down by one.
    /// Only a node nothing hangs on can go: not the root, no children, no
    /// extra parents (unlink them first). Returns what is needed to put it
    /// back exactly: see [`RemovedNode`].
    ///
    /// Time O(V + E).
    ///
    /// # Errors
    ///
    /// On any error the tree is unchanged: [`ModelError::UnknownNode`],
    /// [`ModelError::CannotRemoveRoot`], [`ModelError::NodeHasChildren`],
    /// [`ModelError::NodeIsShared`].
    pub fn remove_leaf(&mut self, idx: NodeIdx) -> Result<RemovedNode, ModelError> {
        let (id, owner, position) = self.check_remove(idx)?;
        if let Some(Node::Gate { children, .. }) = self.nodes.get_mut(owner.index()) {
            children.remove(position);
        }
        self.by_id.remove(&id);
        self.shift(idx.index() + 1, -1);
        let node = self.nodes.remove(idx.index());
        self.ids.remove(idx.index());
        self.owners.remove(idx.index());
        self.linked_parents.remove(idx.index());
        Ok(RemovedNode {
            id,
            node,
            index: idx,
            owner,
            position,
        })
    }

    /// Everything `remove_leaf` checks, and what it needs: the node's ID, its
    /// owner and its position among the owner's children.
    pub(crate) fn check_remove(
        &self,
        idx: NodeIdx,
    ) -> Result<(NodeId, NodeIdx, usize), ModelError> {
        let Some(&id) = self.ids.get(idx.index()) else {
            return Err(ModelError::UnknownNode(idx));
        };
        if idx == self.root {
            return Err(ModelError::CannotRemoveRoot);
        }
        if !self.nodes[idx.index()].children().is_empty() {
            return Err(ModelError::NodeHasChildren(id));
        }
        if !self.linked_parents[idx.index()].is_empty() {
            return Err(ModelError::NodeIsShared(id));
        }
        let owner = self.owners[idx.index()].ok_or(ModelError::CannotRemoveRoot)?;
        let position = self
            .nodes
            .get(owner.index())
            .and_then(|n| n.children().iter().position(|c| *c == idx))
            .ok_or(ModelError::UnknownNode(idx))?;
        Ok((id, owner, position))
    }

    /// Changes the label of `idx` and returns the old one. Time O(1).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownNode`].
    pub fn set_label(&mut self, idx: NodeIdx, label: Arc<str>) -> Result<Arc<str>, ModelError> {
        match self.nodes.get_mut(idx.index()) {
            Some(Node::Gate { label: l, .. } | Node::Step { label: l }) => {
                Ok(std::mem::replace(l, label))
            }
            None => Err(ModelError::UnknownNode(idx)),
        }
    }

    /// Changes how the gate `idx` combines its children and returns the old
    /// kind. Time O(1).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownNode`], [`ModelError::NotAGate`] for a step.
    pub fn set_gate_kind(&mut self, idx: NodeIdx, kind: GateKind) -> Result<GateKind, ModelError> {
        match self.nodes.get_mut(idx.index()) {
            Some(Node::Gate { kind: k, .. }) => Ok(std::mem::replace(k, kind)),
            Some(Node::Step { .. }) => Err(ModelError::NotAGate(idx)),
            None => Err(ModelError::UnknownNode(idx)),
        }
    }

    /// Changes a bare node `idx` between a step (`None`) and an empty gate
    /// (`Some(kind)`), returning the previous gate kind while retaining label,
    /// ID, index, owner, and all incoming edges. Time O(1).
    ///
    /// # Errors
    ///
    /// [`ModelError::UnknownNode`], [`ModelError::NodeHasChildren`] if `idx`
    /// has outgoing children.
    pub fn set_node_kind(
        &mut self,
        idx: NodeIdx,
        gate: Option<GateKind>,
    ) -> Result<Option<GateKind>, ModelError> {
        let Some(id) = self.ids.get(idx.index()).copied() else {
            return Err(ModelError::UnknownNode(idx));
        };
        let Some(slot) = self.nodes.get_mut(idx.index()) else {
            return Err(ModelError::UnknownNode(idx));
        };
        if !slot.children().is_empty() {
            return Err(ModelError::NodeHasChildren(id));
        }
        let (old, label) = match slot {
            Node::Gate { kind, label, .. } => (Some(*kind), Arc::clone(label)),
            Node::Step { label } => (None, Arc::clone(label)),
        };
        *slot = match gate {
            Some(kind) => Node::gate(kind, label),
            None => Node::step(label),
        };
        Ok(old)
    }

    /// Moves every index at or after `from` by `by` (+1 after an insert, -1
    /// after a removal): child lists, owners, extra parents and the ID map.
    /// The root is index 0 and never moves. Time O(1) when appending at the
    /// end of the arena (`from >= len()`), and O(V + E) otherwise.
    fn shift(&mut self, from: usize, by: isize) {
        if from >= self.nodes.len() {
            return;
        }
        let moved = |i: NodeIdx| {
            if i.index() >= from {
                NodeIdx::try_from_usize(i.index().saturating_add_signed(by)).unwrap_or(i)
            } else {
                i
            }
        };
        for node in &mut self.nodes {
            if let Node::Gate { children, .. } = node {
                for c in children {
                    *c = moved(*c);
                }
            }
        }
        for owner in self.owners[from..].iter_mut().flatten() {
            *owner = moved(*owner);
        }
        if self.link_count > 0 {
            for parents in &mut self.linked_parents {
                for p in parents {
                    *p = moved(*p);
                }
            }
        }
        for (offset, id) in self.ids[from..].iter().enumerate() {
            let old_idx = from + offset;
            if let Some(new_idx) = NodeIdx::try_from_usize(old_idx.saturating_add_signed(by))
                && let Some(slot) = self.by_id.get_mut(id)
            {
                *slot = new_idx;
            }
        }
    }

    /// Returns every node in topological order: for every edge
    /// `parent -> child` (from [`add_child`](Self::add_child) or
    /// [`link`](Self::link)), `parent` comes before `child`. This is the
    /// order in which the engine evaluates nodes.
    ///
    /// Uses Kahn's algorithm, without recursion. The order is deterministic:
    /// nodes with no incoming edge start the queue in ascending index (so the
    /// root comes first), and children are visited in their stored order.
    ///
    /// Time O(V + E), extra space O(V) plus the returned `Vec`.
    ///
    /// # Errors
    ///
    /// [`ModelError::Cycle`] with the public IDs of one cycle, in edge order,
    /// if the graph has a cycle. [`link`](Self::link) prevents cycles, so
    /// this is a defense-in-depth check that should not fire.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let r = tree.root();
    /// let g = tree.add_child(r, NodeId::new_v7(), Node::gate(GateKind::And, "g"))?;
    /// let s = tree.add_child(r, NodeId::new_v7(), Node::step("s"))?;
    /// tree.link(g, s)?;
    /// assert_eq!(tree.topological_order()?, vec![r, g, s]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn topological_order(&self) -> Result<Vec<NodeIdx>, ModelError> {
        crate::topo::topological_order(&self.nodes)
            .map_err(|path| ModelError::Cycle(self.ids_for(&path)))
    }

    /// Maps internal indices to public IDs, for error payloads. Indices
    /// produced by the graph algorithms are always in range. Time O(len).
    fn ids_for(&self, path: &[NodeIdx]) -> Vec<NodeId> {
        path.iter().filter_map(|i| self.id_of(*i)).collect()
    }

    /// Returns the index of the root node.
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// assert_eq!(tree.owner(tree.root()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn root(&self) -> NodeIdx {
        self.root
    }

    /// Returns the number of nodes, including the root.
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// assert_eq!(tree.len(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if the tree has no nodes. A tree always has a root, so
    /// this is always `false`; it exists to pair with [`len`](Self::len).
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// assert!(!tree.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the node at `idx`, or `None` if `idx` is not in this tree.
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// assert_eq!(tree.get(tree.root()).map(Node::label), Some("only"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn get(&self, idx: NodeIdx) -> Option<&Node> {
        self.nodes.get(idx.index())
    }

    /// Resolves a public ID to its index, or `None` if the ID is not in this
    /// tree.
    ///
    /// Time O(1) expected (one `HashMap` lookup), no allocation.
    ///
    /// ```
    /// use aegis_model::{AttackTree, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let id = NodeId::new_v7();
    /// let tree = AttackTree::new(id, Node::step("only"))?;
    /// assert_eq!(tree.idx_of(&id), Some(tree.root()));
    /// assert_eq!(tree.idx_of(&NodeId::new_v7()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn idx_of(&self, id: &NodeId) -> Option<NodeIdx> {
        self.by_id.get(id).copied()
    }

    /// Returns the public ID of the node at `idx`, or `None` if `idx` is not
    /// in this tree.
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let id = NodeId::new_v7();
    /// let tree = AttackTree::new(id, Node::step("only"))?;
    /// assert_eq!(tree.id_of(tree.root()), Some(id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn id_of(&self, idx: NodeIdx) -> Option<NodeId> {
        self.ids.get(idx.index()).copied()
    }

    /// Returns the owner of `idx`: the gate it was created under with
    /// [`add_child`](Self::add_child). `None` for the root and for indices not
    /// in this tree. Use [`parents`](Self::parents) for every gate that holds
    /// the node, including those added by [`link`](Self::link).
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let s = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s"))?;
    /// assert_eq!(tree.owner(s), Some(tree.root()));
    /// assert_eq!(tree.owner(tree.root()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn owner(&self, idx: NodeIdx) -> Option<NodeIdx> {
        self.owners.get(idx.index()).copied().flatten()
    }

    /// Iterates over every gate that holds `idx` as a child: the
    /// [`owner`](Self::owner) first, then the gates added by
    /// [`link`](Self::link) in link order (deterministic). Empty for the
    /// root and for indices not in this tree.
    ///
    /// Time O(1) per item, O(number of parents) total; no allocation.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let r = tree.root();
    /// let g = tree.add_child(r, NodeId::new_v7(), Node::gate(GateKind::And, "g"))?;
    /// let s = tree.add_child(g, NodeId::new_v7(), Node::step("s"))?;
    /// tree.link(r, s)?;
    /// assert_eq!(tree.parents(s).collect::<Vec<_>>(), [g, r]);
    /// assert_eq!(tree.parents(r).count(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn parents(&self, idx: NodeIdx) -> impl Iterator<Item = NodeIdx> + '_ {
        let linked = self
            .linked_parents
            .get(idx.index())
            .map_or(&[][..], Vec::as_slice);
        self.owner(idx).into_iter().chain(linked.iter().copied())
    }

    /// Returns the children of `idx` in insertion order. Empty for steps and
    /// for indices not in this tree.
    ///
    /// Time O(1), no allocation.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let s = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s"))?;
    /// assert_eq!(tree.children(tree.root()), &[s]);
    /// assert!(tree.children(s).is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn children(&self, idx: NodeIdx) -> &[NodeIdx] {
        self.get(idx).map_or(&[], Node::children)
    }

    /// Iterates over all nodes in insertion (arena) order, which is
    /// deterministic. The root comes first.
    ///
    /// Time O(1) per item, O(n) total; space O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s"))?;
    /// let labels: Vec<&str> = tree.iter().map(|(_, _, n)| n.label()).collect();
    /// assert_eq!(labels, ["root", "s"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (NodeIdx, NodeId, &Node)> {
        self.nodes
            .iter()
            .zip(&self.ids)
            .enumerate()
            // INVARIANT: every arena position was checked by `try_from_usize`
            // when the node was added, so this never drops an item.
            .filter_map(|(i, (node, id))| NodeIdx::try_from_usize(i).map(|idx| (idx, *id, node)))
    }

    /// Returns the linked (non-owner) parents of `idx` in link order. Empty
    /// for the root, for unshared nodes, and for indices not in this tree.
    /// Time O(1).
    pub(crate) fn linked_parents_of(&self, idx: NodeIdx) -> &[NodeIdx] {
        self.linked_parents
            .get(idx.index())
            .map_or(&[], Vec::as_slice)
    }

    /// Reconstructs an [`AttackTree`] from decoded wire nodes, checking every
    /// structural invariant in O(V + E) time and O(V + E) space:
    /// - at least one node, with unique `NodeId`s;
    /// - every child `NodeId` resolves to a node in this tree, with no
    ///   duplicate child under the same gate;
    /// - the root (index 0) has in-degree 0 and no `parents` list;
    /// - every non-root node `i > 0` has in-degree >= 1, its owner precedes
    ///   it in arena order (`owner.index() < i`), `parents` is omitted when
    ///   in-degree == 1 and present with the exact incoming gate set when
    ///   in-degree >= 2;
    /// - the graph is acyclic ([`topological_order`](Self::topological_order)).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn from_wire_parts(nodes_in: Vec<WireNodeParts>) -> Option<Self> {
        let n = nodes_in.len();
        if n == 0 {
            return None;
        }
        let mut ids = Vec::with_capacity(n);
        let mut by_id = HashMap::with_capacity(n);
        for (i, (id, ..)) in nodes_in.iter().enumerate() {
            let idx = NodeIdx::try_from_usize(i)?;
            if by_id.insert(*id, idx).is_some() {
                return None;
            }
            ids.push(*id);
        }

        let mut nodes = Vec::with_capacity(n);
        let mut incoming: Vec<Vec<NodeIdx>> = vec![Vec::new(); n];
        let mut scratch = vec![false; n];
        for (i, (_, label, gate_children, _)) in nodes_in.iter().enumerate() {
            let p_idx = NodeIdx::try_from_usize(i)?;
            match gate_children {
                None => nodes.push(Node::Step {
                    label: Arc::clone(label),
                }),
                Some((kind, child_ids)) => {
                    let mut children: Vec<NodeIdx> = Vec::with_capacity(child_ids.len());
                    for cid in child_ids {
                        let Some(&c_idx) = by_id.get(cid) else {
                            for c in &children {
                                if let Some(s) = scratch.get_mut(c.index()) {
                                    *s = false;
                                }
                            }
                            return None;
                        };
                        let slot = scratch.get_mut(c_idx.index())?;
                        if *slot {
                            for c in &children {
                                if let Some(s) = scratch.get_mut(c.index()) {
                                    *s = false;
                                }
                            }
                            return None;
                        }
                        *slot = true;
                        children.push(c_idx);
                        incoming.get_mut(c_idx.index())?.push(p_idx);
                    }
                    for c in &children {
                        if let Some(s) = scratch.get_mut(c.index()) {
                            *s = false;
                        }
                    }
                    nodes.push(Node::Gate {
                        kind: *kind,
                        label: Arc::clone(label),
                        children,
                    });
                }
            }
        }

        let mut owners = Vec::with_capacity(n);
        let mut linked_parents = Vec::with_capacity(n);
        for (i, (_, _, _, wire_parents)) in nodes_in.into_iter().enumerate() {
            let inc = incoming.get(i)?;
            if i == 0 {
                if !inc.is_empty() || wire_parents.is_some() {
                    return None;
                }
                owners.push(None);
                linked_parents.push(Vec::new());
            } else if inc.is_empty() {
                return None;
            } else if inc.len() == 1 {
                if wire_parents.is_some() {
                    return None;
                }
                let &owner = inc.first()?;
                if owner.index() >= i {
                    return None;
                }
                owners.push(Some(owner));
                linked_parents.push(Vec::new());
            } else {
                let wp = wire_parents?;
                if wp.len() != inc.len() {
                    return None;
                }
                for &p in inc {
                    *scratch.get_mut(p.index())? = true;
                }
                let mut resolved = Vec::with_capacity(wp.len());
                for pid in &wp {
                    let Some(&p_idx) = by_id.get(pid) else {
                        for &p in inc {
                            if let Some(s) = scratch.get_mut(p.index()) {
                                *s = false;
                            }
                        }
                        return None;
                    };
                    let slot = scratch.get_mut(p_idx.index())?;
                    if !*slot {
                        for &p in inc {
                            if let Some(s) = scratch.get_mut(p.index()) {
                                *s = false;
                            }
                        }
                        return None;
                    }
                    *slot = false;
                    resolved.push(p_idx);
                }
                let &owner = resolved.first()?;
                if owner.index() >= i {
                    return None;
                }
                owners.push(Some(owner));
                linked_parents.push(resolved.into_iter().skip(1).collect());
            }
        }

        crate::topo::topological_order(&nodes).ok()?;
        let root = NodeIdx::try_from_usize(0)?;
        let link_count =
            u32::try_from(linked_parents.iter().map(Vec::len).sum::<usize>()).unwrap_or(u32::MAX);
        Some(Self {
            nodes,
            ids,
            owners,
            linked_parents,
            by_id,
            root,
            link_count,
        })
    }

    /// Total number of directed edges (`parent -> child`) in this tree.
    ///
    /// Time O(1).
    #[must_use]
    pub(crate) fn edge_count(&self) -> usize {
        self.len().saturating_sub(1) + (self.link_count as usize)
    }

    /// Checks every structural invariant. Used by unit tests. Time O(n).
    #[cfg(test)]
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        let n = self.nodes.len();
        if self.ids.len() != n
            || self.owners.len() != n
            || self.linked_parents.len() != n
            || self.by_id.len() != n
        {
            return Err(format!(
                "length mismatch: nodes {n}, ids {}, owners {}, linked {}, by_id {}",
                self.ids.len(),
                self.owners.len(),
                self.linked_parents.len(),
                self.by_id.len()
            ));
        }
        for (i, id) in self.ids.iter().enumerate() {
            if self.by_id.get(id).map(|x| x.index()) != Some(i) {
                return Err(format!("by_id[ids[{i}]] != {i}"));
            }
            match self.owners[i] {
                None if i == self.root.index() => {}
                None => return Err(format!("non-root {i} has no parent")),
                Some(_) if i == self.root.index() => return Err("root has a parent".into()),
                Some(p) => {
                    let count = self.children(p).iter().filter(|c| c.index() == i).count();
                    if count != 1 {
                        return Err(format!("{i} appears {count} times in parent's children"));
                    }
                }
            }
            let children = self.nodes[i].children();
            for (k, c) in children.iter().enumerate() {
                if c.index() >= n {
                    return Err(format!("child {} of {i} is not in the tree", c.index()));
                }
                if children[..k].contains(c) {
                    return Err(format!("child {} appears twice under {i}", c.index()));
                }
                // Edge i -> c: i is in parents(c) exactly once.
                let count = self.parents(*c).filter(|p| p.index() == i).count();
                if count != 1 {
                    return Err(format!(
                        "{i} appears {count} times in parents({})",
                        c.index()
                    ));
                }
            }
            // Reverse: every listed parent holds i as a child exactly once.
            let Some(idx) = NodeIdx::try_from_usize(i) else {
                return Err(format!("index {i} out of range"));
            };
            for p in self.parents(idx) {
                let count = self.children(p).iter().filter(|c| c.index() == i).count();
                if count != 1 {
                    return Err(format!(
                        "{i} appears {count} times in children({})",
                        p.index()
                    ));
                }
            }
        }
        let expected_links: usize = self.linked_parents.iter().map(Vec::len).sum();
        if self.link_count as usize != expected_links {
            return Err(format!(
                "link_count {} != linked_parents sum {expected_links}",
                self.link_count
            ));
        }
        if let Err(e) = self.topological_order() {
            return Err(format!("graph has a cycle: {e}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn gate_tree() -> AttackTree {
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).expect("valid root")
    }

    #[test]
    fn new_rejects_root_with_children() {
        let mut root = Node::gate(GateKind::And, "root");
        if let Node::Gate { children, .. } = &mut root {
            children.push(gate_tree().root());
        }
        assert_eq!(
            AttackTree::new(NodeId::new_v7(), root).map(|t| t.len()),
            Err(ModelError::ChildrenNotEmpty)
        );
    }

    #[test]
    fn new_tree_has_only_root() {
        let id = NodeId::new_v7();
        let tree = AttackTree::new(id, Node::step("r")).expect("valid root");
        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());
        assert_eq!(tree.root().index(), 0);
        assert_eq!(tree.id_of(tree.root()), Some(id));
        assert_eq!(tree.idx_of(&id), Some(tree.root()));
        assert_eq!(tree.owner(tree.root()), None);
        assert_eq!(tree.children(tree.root()), []);
        tree.check_invariants().expect("invariants");
    }

    #[test]
    fn lookups_on_unknown_index_return_nothing() {
        let mut big = gate_tree();
        let far = big
            .add_child(big.root(), NodeId::new_v7(), Node::step("x"))
            .expect("added");
        let small = gate_tree();
        assert_eq!(small.get(far), None);
        assert_eq!(small.id_of(far), None);
        assert_eq!(small.owner(far), None);
        assert_eq!(small.children(far), []);
    }

    #[test]
    fn node_accessors() {
        let g = Node::gate(GateKind::SeqAnd, "g");
        assert_eq!(g.label(), "g");
        assert_eq!(g.gate_kind(), Some(GateKind::SeqAnd));
        assert_eq!(g.children(), []);
        let s = Node::step(Arc::<str>::from("s"));
        assert_eq!(s.label(), "s");
        assert_eq!(s.gate_kind(), None);
        assert_eq!(s.children(), []);
    }

    #[test]
    fn shared_label_is_not_copied() {
        let label: Arc<str> = Arc::from("shared");
        let a = Node::step(Arc::clone(&label));
        let b = Node::step(Arc::clone(&label));
        assert!(std::ptr::eq(a.label(), b.label()));
    }

    #[test]
    fn children_keep_insertion_order() {
        let mut tree = gate_tree();
        let r = tree.root();
        let kids: Vec<NodeIdx> = (0..5)
            .map(|i| {
                tree.add_child(r, NodeId::new_v7(), Node::step(format!("s{i}")))
                    .expect("added")
            })
            .collect();
        assert_eq!(tree.children(r), kids.as_slice());
    }

    #[test]
    fn iter_is_insertion_order() {
        let mut tree = gate_tree();
        let r = tree.root();
        let a = tree
            .add_child(r, NodeId::new_v7(), Node::gate(GateKind::And, "a"))
            .expect("added");
        tree.add_child(a, NodeId::new_v7(), Node::step("b"))
            .expect("added");
        tree.add_child(r, NodeId::new_v7(), Node::step("c"))
            .expect("added");
        let seen: Vec<(usize, &str)> = tree
            .iter()
            .map(|(i, id, n)| {
                assert_eq!(tree.idx_of(&id), Some(i));
                (i.index(), n.label())
            })
            .collect();
        assert_eq!(seen, [(0, "root"), (1, "a"), (2, "b"), (3, "c")]);
    }
}

#[cfg(test)]
mod small_tree {
    use super::*;

    /// Builds the spike's 13-node reference shape (`spikes/shared/tree.json`):
    /// g0 OR [ g1 AND [s1..s4], g2 AND [s5..s7], g3 OR [s8, s9] ].
    fn build() -> (AttackTree, Vec<(&'static str, NodeIdx)>) {
        let mut out = Vec::new();
        let mut tree =
            AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "g0")).expect("root");
        let r = tree.root();
        out.push(("g0", r));
        let groups: [(&str, GateKind, &[&str]); 3] = [
            ("g1", GateKind::And, &["s1", "s2", "s3", "s4"]),
            ("g2", GateKind::And, &["s5", "s6", "s7"]),
            ("g3", GateKind::Or, &["s8", "s9"]),
        ];
        for (g, kind, steps) in groups {
            let gi = tree
                .add_child(r, NodeId::new_v7(), Node::gate(kind, g))
                .expect("gate");
            out.push((g, gi));
            for s in steps {
                let si = tree
                    .add_child(gi, NodeId::new_v7(), Node::step(*s))
                    .expect("step");
                out.push((s, si));
            }
        }
        (tree, out)
    }

    #[test]
    fn small_tree_shape() {
        let (tree, nodes) = build();
        assert_eq!(tree.len(), 13);
        tree.check_invariants().expect("invariants");
        let find = |name: &str| {
            nodes
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, i)| *i)
                .expect("known name")
        };
        let g0 = find("g0");
        assert_eq!(tree.owner(g0), None);
        assert_eq!(tree.children(g0), &[find("g1"), find("g2"), find("g3")]);
        let expect_children = [
            ("g1", vec!["s1", "s2", "s3", "s4"]),
            ("g2", vec!["s5", "s6", "s7"]),
            ("g3", vec!["s8", "s9"]),
        ];
        for (g, steps) in expect_children {
            let gi = find(g);
            assert_eq!(tree.owner(gi), Some(g0));
            let want: Vec<NodeIdx> = steps.iter().map(|s| find(s)).collect();
            assert_eq!(tree.children(gi), want.as_slice());
            for s in want {
                assert_eq!(tree.owner(s), Some(gi));
                assert_eq!(tree.children(s), []);
            }
        }
        for (name, i) in &nodes {
            assert_eq!(tree.get(*i).map(Node::label), Some(*name));
        }
    }

    #[test]
    fn small_tree_id_round_trip() {
        let (tree, _) = build();
        for (i, _, _) in tree.iter() {
            let id = tree.id_of(i).expect("id");
            assert_eq!(tree.idx_of(&id), Some(i));
        }
    }
}

#[cfg(test)]
mod errors {
    use super::*;

    /// Observable state used to show that a failed call changed nothing.
    fn snapshot(tree: &AttackTree) -> Vec<(NodeIdx, NodeId, Node, Option<NodeIdx>)> {
        tree.iter()
            .map(|(i, id, n)| (i, id, n.clone(), tree.owner(i)))
            .collect()
    }

    fn assert_unchanged(tree: &AttackTree, before: &[(NodeIdx, NodeId, Node, Option<NodeIdx>)]) {
        assert_eq!(tree.len(), before.len());
        assert_eq!(snapshot(tree), before);
        for (i, id, _, _) in before {
            assert_eq!(tree.idx_of(id), Some(*i));
        }
        tree.check_invariants().expect("invariants");
    }

    fn setup() -> (AttackTree, NodeIdx, NodeId) {
        let mut tree =
            AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).expect("root");
        let sid = NodeId::new_v7();
        let step = tree
            .add_child(tree.root(), sid, Node::step("s"))
            .expect("step");
        (tree, step, sid)
    }

    #[test]
    fn duplicate_id() {
        let (mut tree, _, sid) = setup();
        let before = snapshot(&tree);
        let root_id = tree.id_of(tree.root()).expect("root id");
        for dup in [sid, root_id] {
            assert_eq!(
                tree.add_child(tree.root(), dup, Node::step("dup")),
                Err(ModelError::DuplicateId(dup))
            );
            assert_unchanged(&tree, &before);
        }
    }

    #[test]
    fn not_a_gate() {
        let (mut tree, step, _) = setup();
        let before = snapshot(&tree);
        let fresh = NodeId::new_v7();
        assert_eq!(
            tree.add_child(step, fresh, Node::step("x")),
            Err(ModelError::NotAGate(step))
        );
        assert_unchanged(&tree, &before);
        assert_eq!(tree.idx_of(&fresh), None);
    }

    #[test]
    fn unknown_node_from_larger_tree() {
        let (mut tree, _, _) = setup();
        let mut larger =
            AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "big")).expect("root");
        let mut last = larger.root();
        for _ in 0..5 {
            last = larger
                .add_child(
                    larger.root(),
                    NodeId::new_v7(),
                    Node::gate(GateKind::Or, "g"),
                )
                .expect("gate");
        }
        let before = snapshot(&tree);
        let fresh = NodeId::new_v7();
        assert_eq!(
            tree.add_child(last, fresh, Node::step("x")),
            Err(ModelError::UnknownNode(last))
        );
        assert_unchanged(&tree, &before);
        assert_eq!(tree.idx_of(&fresh), None);
    }

    #[test]
    fn children_not_empty() {
        let (mut tree, step, _) = setup();
        let before = snapshot(&tree);
        let fresh = NodeId::new_v7();
        let node = Node::Gate {
            kind: GateKind::And,
            label: Arc::from("g"),
            children: vec![step],
        };
        assert_eq!(
            tree.add_child(tree.root(), fresh, node),
            Err(ModelError::ChildrenNotEmpty)
        );
        assert_unchanged(&tree, &before);
        assert_eq!(tree.idx_of(&fresh), None);
    }

    #[test]
    fn error_messages_are_readable() {
        let (_, step, sid) = setup();
        assert_eq!(
            ModelError::NotAGate(step).to_string(),
            "node index 1 is a step and cannot have children"
        );
        assert!(
            ModelError::DuplicateId(sid)
                .to_string()
                .contains(&sid.to_string())
        );
    }

    #[test]
    fn set_node_kind_converts_bare_root_and_shared_nodes_retaining_ids_and_parents() {
        let root_id = NodeId::new_v7();
        let g1_id = NodeId::new_v7();
        let g2_id = NodeId::new_v7();
        let s0_id = NodeId::new_v7();
        let m_id = NodeId::new_v7();
        let s2_id = NodeId::new_v7();
        let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "root")).expect("root");
        let r = tree.root();
        let g1 = tree
            .add_child(r, g1_id, Node::gate(GateKind::And, "g1"))
            .expect("g1");
        let g2 = tree
            .add_child(r, g2_id, Node::gate(GateKind::Or, "g2"))
            .expect("g2");
        let s0 = tree.add_child(g1, s0_id, Node::step("s0")).expect("s0");
        let m = tree.add_child(g1, m_id, Node::step("shared")).expect("m");
        let s2 = tree.add_child(g1, s2_id, Node::step("s2")).expect("s2");
        tree.link(g2, m).expect("link m under g2");

        // Non-root multiply-parented Step -> empty Gate -> Step, retaining ID, index, owner, parents.
        assert_eq!(tree.set_node_kind(m, Some(GateKind::SeqAnd)), Ok(None));
        assert_eq!(tree.idx_of(&m_id), Some(m));
        assert_eq!(tree.id_of(m), Some(m_id));
        assert_eq!(tree.get(m).map(Node::label), Some("shared"));
        assert_eq!(
            tree.get(m).and_then(Node::gate_kind),
            Some(GateKind::SeqAnd)
        );
        assert_eq!(tree.owner(m), Some(g1));
        assert_eq!(tree.parents(m).collect::<Vec<_>>(), [g1, g2]);
        assert_eq!(tree.children(g1), &[s0, m, s2]);
        assert_eq!(tree.children(g2), &[m]);
        tree.check_invariants().expect("invariants");

        assert_eq!(tree.set_node_kind(m, None), Ok(Some(GateKind::SeqAnd)));
        assert_eq!(tree.get(m).and_then(Node::gate_kind), None);
        assert_eq!(tree.parents(m).collect::<Vec<_>>(), [g1, g2]);
        assert_eq!(tree.children(g1), &[s0, m, s2]);
        tree.check_invariants().expect("invariants");

        // Child-bearing gate and unknown node are rejected without mutation.
        let before = snapshot(&tree);
        assert_eq!(
            tree.set_node_kind(g1, None),
            Err(ModelError::NodeHasChildren(g1_id))
        );
        assert_eq!(
            tree.set_node_kind(g1, Some(GateKind::Or)),
            Err(ModelError::NodeHasChildren(g1_id))
        );
        let missing = NodeIdx::try_from_usize(999).expect("idx");
        assert_eq!(
            tree.set_node_kind(missing, None),
            Err(ModelError::UnknownNode(missing))
        );
        assert_unchanged(&tree, &before);

        // Single-node root Step <-> Gate.
        let mut solo = AttackTree::new(root_id, Node::step("solo")).expect("solo");
        let sr = solo.root();
        assert_eq!(solo.set_node_kind(sr, Some(GateKind::And)), Ok(None));
        assert_eq!(solo.get(sr).and_then(Node::gate_kind), Some(GateKind::And));
        assert_eq!(solo.id_of(sr), Some(root_id));
        assert_eq!(solo.get(sr).map(Node::label), Some("solo"));
        assert_eq!(solo.set_node_kind(sr, None), Ok(Some(GateKind::And)));
        assert_eq!(solo.get(sr).and_then(Node::gate_kind), None);
        solo.check_invariants().expect("solo invariants");
    }
}

#[cfg(test)]
mod large_tree {
    use super::*;

    /// `large` reference size from the engineering standards.
    const N: usize = 100_000;

    #[test]
    fn large_tree_builds_and_resolves() {
        let mut tree =
            AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).expect("root");
        let kinds = [GateKind::And, GateKind::Or, GateKind::SeqAnd];
        let step_label: Arc<str> = Arc::from("step");
        let gate_label: Arc<str> = Arc::from("gate");
        let mut gates = vec![tree.root()];
        let mut ids = vec![tree.id_of(tree.root()).expect("root id")];
        // Deterministic pseudo-random parent choice (Knuth multiplicative hash).
        for i in 1..N {
            let parent = gates[(i.wrapping_mul(2_654_435_761)) % gates.len()];
            let id = NodeId::new_v7();
            let node = if i % 4 == 0 {
                Node::step(Arc::clone(&step_label))
            } else {
                Node::Gate {
                    kind: kinds[i % 3],
                    label: Arc::clone(&gate_label),
                    children: Vec::new(),
                }
            };
            let is_gate = node.gate_kind().is_some();
            let idx = tree.add_child(parent, id, node).expect("add");
            assert_eq!(idx.index(), i);
            if is_gate {
                gates.push(idx);
            }
            ids.push(id);
        }
        assert_eq!(tree.len(), N);
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(tree.idx_of(id).map(NodeIdx::index), Some(i));
        }
        tree.check_invariants().expect("invariants");
    }
}

#[cfg(test)]
mod links {
    use super::*;

    fn new_tree() -> AttackTree {
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root")).expect("root")
    }

    fn gate(tree: &mut AttackTree, parent: NodeIdx, label: &str) -> NodeIdx {
        tree.add_child(parent, NodeId::new_v7(), Node::gate(GateKind::And, label))
            .expect("gate")
    }

    fn step(tree: &mut AttackTree, parent: NodeIdx, label: &str) -> NodeIdx {
        tree.add_child(parent, NodeId::new_v7(), Node::step(label))
            .expect("step")
    }

    fn id(tree: &AttackTree, i: NodeIdx) -> NodeId {
        tree.id_of(i).expect("id")
    }

    fn ids(tree: &AttackTree, path: &[NodeIdx]) -> Vec<NodeId> {
        path.iter().map(|i| id(tree, *i)).collect()
    }

    /// Children and parents of every node, to show a failed call changed
    /// nothing.
    fn edges(tree: &AttackTree) -> Vec<(Vec<NodeIdx>, Vec<NodeIdx>)> {
        tree.iter()
            .map(|(i, _, _)| (tree.children(i).to_vec(), tree.parents(i).collect()))
            .collect()
    }

    /// Bypasses `link` to put a cycle into the arena, so the
    /// defense-in-depth path of `topological_order` can be tested.
    fn force_edge(tree: &mut AttackTree, p: NodeIdx, c: NodeIdx) {
        if let Node::Gate { children, .. } = &mut tree.nodes[p.index()] {
            children.push(c);
        }
        tree.linked_parents[c.index()].push(p);
    }

    /// Asserts that `link(p, c)` fails with `WouldCreateCycle` carrying
    /// `path`, and that the tree is unchanged.
    fn assert_cycle_rejected(tree: &mut AttackTree, p: NodeIdx, c: NodeIdx, path: &[NodeIdx]) {
        let before = edges(tree);
        let len = tree.len();
        assert_eq!(
            tree.link(p, c),
            Err(ModelError::WouldCreateCycle {
                parent: id(tree, p),
                child: id(tree, c),
                path: ids(tree, path),
            }),
            "link({p:?}, {c:?})"
        );
        assert_eq!(tree.len(), len);
        assert_eq!(edges(tree), before);
        tree.check_invariants().expect("invariants");
    }

    /// Checks that `order` is a permutation of all nodes that respects every
    /// edge.
    fn assert_valid_order(tree: &AttackTree, order: &[NodeIdx]) {
        assert_eq!(order.len(), tree.len());
        let mut pos = vec![usize::MAX; tree.len()];
        for (k, n) in order.iter().enumerate() {
            assert_eq!(pos[n.index()], usize::MAX, "{} repeated", n.index());
            pos[n.index()] = k;
        }
        for (p, _, node) in tree.iter() {
            for c in node.children() {
                assert!(
                    pos[p.index()] < pos[c.index()],
                    "{p:?} -> {c:?} out of order"
                );
            }
        }
    }

    #[test]
    fn link_shares_child() {
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, r, "g2");
        let other = step(&mut tree, g2, "other");
        let s = step(&mut tree, g1, "s");
        assert_eq!(tree.link(g2, s), Ok(()));
        assert_eq!(tree.children(g2), &[other, s]);
        assert_eq!(tree.children(g1), &[s]);
        assert_eq!(tree.owner(s), Some(g1));
        assert_eq!(tree.parents(s).collect::<Vec<_>>(), [g1, g2]);
        assert_eq!(tree.len(), 5);
        tree.check_invariants().expect("invariants");
    }

    #[test]
    fn owner_and_parents() {
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, r, "g2");
        let g3 = gate(&mut tree, r, "g3");
        let s = step(&mut tree, g2, "s");
        tree.link(g3, s).expect("link");
        tree.link(g1, s).expect("link");
        tree.link(r, s).expect("link");
        // Owner first, then linked parents in link order (not index order).
        assert_eq!(tree.parents(s).collect::<Vec<_>>(), [g2, g3, g1, r]);
        assert_eq!(tree.owner(s), Some(g2));
        assert_eq!(tree.parents(g1).collect::<Vec<_>>(), [r]);
        assert_eq!(tree.parents(r).count(), 0);
        assert_eq!(tree.owner(r), None);
        // Index from a larger tree: empty, no panic.
        let mut big = new_tree();
        let big_root = big.root();
        let mut far = big_root;
        for _ in 0..10 {
            far = step(&mut big, big_root, "x");
        }
        assert_eq!(tree.parents(far).count(), 0);
        assert_eq!(tree.owner(far), None);
        tree.check_invariants().expect("invariants");
    }

    #[test]
    fn link_errors() {
        let mut tree = new_tree();
        let r = tree.root();
        let g = gate(&mut tree, r, "g");
        let s = step(&mut tree, g, "s");
        // An index valid only in a larger tree.
        let mut big = new_tree();
        let big_root = big.root();
        let mut far = big_root;
        for _ in 0..10 {
            far = step(&mut big, big_root, "x");
        }
        let (rid, gid, sid) = (id(&tree, r), id(&tree, g), id(&tree, s));
        let before_len = tree.len();
        let before = edges(&tree);
        let cases = [
            ((far, s), ModelError::UnknownNode(far)),
            // Both unknown: the parent is reported first.
            ((far, far), ModelError::UnknownNode(far)),
            ((g, far), ModelError::UnknownNode(far)),
            // Step parent and unknown child: the child is reported first.
            ((s, far), ModelError::UnknownNode(far)),
            ((s, g), ModelError::NotAGate(s)),
            ((s, s), ModelError::NotAGate(s)),
            (
                (g, s),
                ModelError::DuplicateEdge {
                    parent: gid,
                    child: sid,
                },
            ),
            (
                (r, g),
                ModelError::DuplicateEdge {
                    parent: rid,
                    child: gid,
                },
            ),
        ];
        for ((p, c), want) in cases {
            assert_eq!(tree.link(p, c), Err(want), "link({p:?}, {c:?})");
            assert_eq!(tree.len(), before_len);
            assert_eq!(edges(&tree), before);
            tree.check_invariants().expect("invariants");
        }
        // A linked edge is also a duplicate.
        tree.link(r, s).expect("link");
        let before = edges(&tree);
        assert_eq!(
            tree.link(r, s),
            Err(ModelError::DuplicateEdge {
                parent: rid,
                child: sid
            })
        );
        assert_eq!(edges(&tree), before);
    }

    #[test]
    fn link_rejects_self_link() {
        let mut tree = new_tree();
        let r = tree.root();
        let a = gate(&mut tree, r, "a");
        step(&mut tree, a, "below");
        assert_cycle_rejected(&mut tree, a, a, &[a]);
        assert_cycle_rejected(&mut tree, r, r, &[r]);
    }

    #[test]
    fn link_rejects_descendant() {
        // A -> B -> C; link(C, A) would close A -> B -> C -> A.
        let mut tree = new_tree();
        let r = tree.root();
        let a = gate(&mut tree, r, "A");
        let b = gate(&mut tree, a, "B");
        let c = gate(&mut tree, b, "C");
        assert_cycle_rejected(&mut tree, c, a, &[a, b, c]);
        assert_cycle_rejected(&mut tree, c, b, &[b, c]);
        assert_cycle_rejected(&mut tree, b, a, &[a, b]);
    }

    #[test]
    fn link_rejects_root() {
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, g1, "g2");
        let g3 = gate(&mut tree, r, "g3");
        assert_cycle_rejected(&mut tree, g1, r, &[r, g1]);
        assert_cycle_rejected(&mut tree, g2, r, &[r, g1, g2]);
        assert_cycle_rejected(&mut tree, g3, r, &[r, g3]);
    }

    #[test]
    fn link_rejects_cycle_through_link() {
        // r -> a, r -> b; link(a, b), then link(b, a) closes a -> b -> a via
        // the linked edge.
        let mut tree = new_tree();
        let r = tree.root();
        let a = gate(&mut tree, r, "a");
        let b = gate(&mut tree, r, "b");
        let c = gate(&mut tree, b, "c");
        tree.link(a, b).expect("link");
        assert_cycle_rejected(&mut tree, c, a, &[a, b, c]);
        assert_cycle_rejected(&mut tree, b, a, &[a, b]);
    }

    #[test]
    fn link_allows_diamond() {
        // Two gates share one child; the shared child has its own subtree.
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, r, "g2");
        let shared = gate(&mut tree, g1, "shared");
        let leaf = step(&mut tree, shared, "leaf");
        assert_eq!(tree.link(g2, shared), Ok(()));
        assert_eq!(tree.parents(shared).collect::<Vec<_>>(), [g1, g2]);
        assert_eq!(tree.parents(leaf).collect::<Vec<_>>(), [shared]);
        tree.check_invariants().expect("invariants");
    }

    #[test]
    fn link_allows_shortcut() {
        // A -> B -> C; link(A, C) adds a shortcut to an existing descendant.
        let mut tree = new_tree();
        let r = tree.root();
        let a = gate(&mut tree, r, "A");
        let b = gate(&mut tree, a, "B");
        let c = gate(&mut tree, b, "C");
        assert_eq!(tree.link(a, c), Ok(()));
        assert_eq!(tree.link(r, c), Ok(()));
        assert_eq!(tree.children(a), &[b, c]);
        assert_eq!(tree.parents(c).collect::<Vec<_>>(), [b, a, r]);
        assert_valid_order(&tree, &tree.topological_order().expect("acyclic"));
        tree.check_invariants().expect("invariants");
    }

    #[test]
    fn topo_order_acyclic() {
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, r, "g2");
        let shared = gate(&mut tree, g1, "shared");
        let leaf = step(&mut tree, shared, "leaf");
        let s2 = step(&mut tree, g2, "s2");
        tree.link(g2, shared).expect("link");
        // A later gate linking an earlier node: g3 must still precede g1.
        let g3 = gate(&mut tree, g2, "g3");
        tree.link(g3, g1).expect("link");
        tree.link(g3, leaf).expect("link");

        let order = tree.topological_order().expect("acyclic");
        assert_eq!(order.first(), Some(&r));
        assert_valid_order(&tree, &order);
        assert_eq!(order, vec![r, g2, s2, g3, g1, shared, leaf]);

        // Single-node tree.
        let one = new_tree();
        assert_eq!(one.topological_order(), Ok(vec![one.root()]));
    }

    #[test]
    fn cycle_self_link() {
        let mut tree = new_tree();
        let r = tree.root();
        let a = gate(&mut tree, r, "a");
        step(&mut tree, a, "below");
        force_edge(&mut tree, a, a);
        assert_eq!(
            tree.topological_order(),
            Err(ModelError::Cycle(vec![id(&tree, a)]))
        );
        assert!(tree.check_invariants().is_err());
    }

    #[test]
    fn cycle_path_exact() {
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, g1, "g2");
        let g3 = gate(&mut tree, g2, "g3");
        // Nodes hanging below the cycle.
        let below = gate(&mut tree, g3, "below");
        step(&mut tree, below, "s1");
        step(&mut tree, g2, "s2");
        step(&mut tree, r, "s3");
        force_edge(&mut tree, g3, g1);
        assert_eq!(
            tree.topological_order(),
            Err(ModelError::Cycle(ids(&tree, &[g1, g2, g3])))
        );
    }

    #[test]
    fn topo_deterministic() {
        let mut tree = new_tree();
        let r = tree.root();
        let g1 = gate(&mut tree, r, "g1");
        let g2 = gate(&mut tree, r, "g2");
        let s = step(&mut tree, g1, "s");
        tree.link(g2, s).expect("link");
        let first = tree.topological_order();
        assert!(first.is_ok());
        assert_eq!(tree.topological_order(), first);
        assert_eq!(tree.clone().topological_order(), first);

        force_edge(&mut tree, g2, g1);
        force_edge(&mut tree, g1, g2);
        let first = tree.topological_order();
        assert!(first.is_err());
        assert_eq!(tree.topological_order(), first);
        assert_eq!(tree.clone().topological_order(), first);
    }

    #[test]
    fn cycle_display() {
        let mut tree = new_tree();
        let r = tree.root();
        let a = gate(&mut tree, r, "secret label a");
        let b = gate(&mut tree, a, "secret label b");
        let (ri, ai, bi) = (id(&tree, r), id(&tree, a), id(&tree, b));

        assert_eq!(
            ModelError::Cycle(vec![ai, bi, ri]).to_string(),
            format!("cycle in attack tree: {ai} -> {bi} -> {ri} -> {ai}")
        );
        assert_eq!(
            ModelError::Cycle(vec![ai]).to_string(),
            format!("cycle in attack tree: {ai} -> {ai}")
        );

        let err = tree.link(b, r).expect_err("cycle");
        let text = err.to_string();
        assert_eq!(
            text,
            format!(
                "linking node {ri} under node {bi} would create a cycle: \
                 {bi} -> {ri} -> {ai} -> {bi}"
            )
        );
        let dup = tree.link(r, a).expect_err("duplicate").to_string();
        assert_eq!(dup, format!("node {ai} is already a child of node {ri}"));
        // Labels are customer data and never appear in errors.
        for t in [&text, &dup] {
            assert!(!t.contains("secret"), "{t}");
            assert!(!t.contains("root"), "{t}");
        }
        // Hyphenated UUIDs, as `NodeId` displays them.
        assert!(text.contains(&ri.as_uuid().hyphenated().to_string()));
    }

    #[test]
    fn invariants_catch_missing_parent_entry() {
        let mut tree = new_tree();
        let r = tree.root();
        let g = gate(&mut tree, r, "g");
        let s = step(&mut tree, g, "s");
        if let Node::Gate { children, .. } = &mut tree.nodes[r.index()] {
            children.push(s);
        }
        assert!(tree.check_invariants().is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_tree_equality_and_inequality() {
        let root_id = NodeId::new_v7();
        let sub_id = NodeId::new_v7();
        let step_id = NodeId::new_v7();

        let build = |root_kind: GateKind, b_label: &str| {
            let mut tree = AttackTree::new(root_id, Node::gate(root_kind, "root")).expect("root");
            let gate = tree
                .add_child(tree.root(), sub_id, Node::gate(GateKind::And, "sub"))
                .expect("a");
            let step = tree
                .add_child(gate, step_id, Node::step(b_label))
                .expect("b");
            tree.link(tree.root(), step).expect("link");
            tree
        };

        let t1 = build(GateKind::Or, "step");
        let t2 = build(GateKind::Or, "step");
        assert_eq!(t1, t2);
        assert_eq!(t1, t1.clone());

        let diff_gate = build(GateKind::And, "step");
        assert_ne!(t1, diff_gate);

        let diff_label = build(GateKind::Or, "other");
        assert_ne!(t1, diff_label);

        let mut no_link = AttackTree::new(root_id, Node::gate(GateKind::Or, "root")).expect("root");
        let gate = no_link
            .add_child(no_link.root(), sub_id, Node::gate(GateKind::And, "sub"))
            .expect("a");
        no_link
            .add_child(gate, step_id, Node::step("step"))
            .expect("b");
        assert_ne!(t1, no_link);
    }
}
