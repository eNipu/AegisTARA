//! Arena-based attack tree.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ModelError;
use crate::id::{NodeId, NodeIdx};

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
/// Nodes are only ever added, each one already attached to its parent, so
/// the structure is always a tree and never has cycles.
///
/// Invariants, kept by every method:
/// - `nodes`, `ids`, `parents`, and `by_id` all have the same length;
/// - `by_id[ids[i]] == i` for every index `i`;
/// - the root has no parent; every other node has exactly one parent and
///   appears exactly once in that parent's children.
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
#[derive(Clone, Debug)]
pub struct AttackTree {
    /// Arena; position = `NodeIdx`.
    nodes: Vec<Node>,
    /// Index to public ID (parallel to `nodes`).
    ids: Vec<NodeId>,
    /// Index to parent (parallel to `nodes`); `None` only for the root.
    parents: Vec<Option<NodeIdx>>,
    /// Public ID to index. Never iterated for output; order comes from the
    /// arena.
    by_id: HashMap<NodeId, NodeIdx>,
    /// Index of the root node.
    root: NodeIdx,
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
            parents: vec![None],
            by_id,
            root: root_idx,
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
    /// assert_eq!(tree.parent(step), Some(tree.root()));
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
        self.parents.push(Some(parent));
        self.by_id.insert(id, idx);
        Ok(idx)
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
    /// assert_eq!(tree.parent(tree.root()), None);
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

    /// Returns the parent of `idx`, or `None` for the root and for indices
    /// not in this tree.
    ///
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let s = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s"))?;
    /// assert_eq!(tree.parent(s), Some(tree.root()));
    /// assert_eq!(tree.parent(tree.root()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn parent(&self, idx: NodeIdx) -> Option<NodeIdx> {
        self.parents.get(idx.index()).copied().flatten()
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

    /// Checks every structural invariant. Used by unit tests. Time O(n).
    #[cfg(test)]
    pub(crate) fn check_invariants(&self) -> Result<(), String> {
        let n = self.nodes.len();
        if self.ids.len() != n || self.parents.len() != n || self.by_id.len() != n {
            return Err(format!(
                "length mismatch: nodes {n}, ids {}, parents {}, by_id {}",
                self.ids.len(),
                self.parents.len(),
                self.by_id.len()
            ));
        }
        for (i, id) in self.ids.iter().enumerate() {
            if self.by_id.get(id).map(|x| x.index()) != Some(i) {
                return Err(format!("by_id[ids[{i}]] != {i}"));
            }
            match self.parents[i] {
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
            for c in self.nodes[i].children() {
                if self
                    .parents
                    .get(c.index())
                    .copied()
                    .flatten()
                    .map(NodeIdx::index)
                    != Some(i)
                {
                    return Err(format!("child {} of {i} has another parent", c.index()));
                }
            }
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
        assert_eq!(tree.parent(tree.root()), None);
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
        assert_eq!(small.parent(far), None);
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
        assert_eq!(tree.parent(g0), None);
        assert_eq!(tree.children(g0), &[find("g1"), find("g2"), find("g3")]);
        let expect_children = [
            ("g1", vec!["s1", "s2", "s3", "s4"]),
            ("g2", vec!["s5", "s6", "s7"]),
            ("g3", vec!["s8", "s9"]),
        ];
        for (g, steps) in expect_children {
            let gi = find(g);
            assert_eq!(tree.parent(gi), Some(g0));
            let want: Vec<NodeIdx> = steps.iter().map(|s| find(s)).collect();
            assert_eq!(tree.children(gi), want.as_slice());
            for s in want {
                assert_eq!(tree.parent(s), Some(gi));
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
            .map(|(i, id, n)| (i, id, n.clone(), tree.parent(i)))
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
