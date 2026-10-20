//! Bottom-up evaluation of an attack tree over a pluggable attribute domain.
//!
//! An attribute domain (Mauw and Oostdijk, "Foundations of Attack Trees",
//! ICISC 2005) says how each gate kind combines the values of its children,
//! for example "OR = most feasible child, AND = least feasible child". The
//! caller supplies the value of every leaf step. [`evaluate`] computes a
//! value for every node in one pass over a topological order, so each node
//! is evaluated exactly once, also when subtrees are shared (rule A2).
//!
//! Every evaluation also records how each value was produced, one
//! [`Derivation`] per node: supplied by the caller (a step), selected from
//! one named child by the gate's rule, or combined from several children.
//! [`Evaluation::explain`] and [`Evaluation::why_path`] read this trace, so
//! any number can explain itself ("OR takes the most feasible child: S7").
//!
//! This module uses only the public API of [`AttackTree`], so it can move to
//! a later engine crate unchanged. See `docs/adr/0005-tree-evaluation.md`
//! and `docs/adr/0006-explain-why-traces.md`.

use std::fmt;
use std::marker::PhantomData;

use crate::error::ModelError;
use crate::id::NodeIdx;
use crate::tree::{AttackTree, GateKind, Node};

/// How gate values combine: one binary operation per gate kind.
///
/// A gate with children `c1, ..., ck` (in stored order, see
/// [`AttackTree::children`]) gets the value of a left fold:
/// `op(...op(op(v1.clone(), &v2), &v3)..., &vk)`, where `op` is
/// [`or`](Self::or), [`and`](Self::and), or [`seq_and`](Self::seq_and) for
/// [`GateKind::Or`], [`GateKind::And`], and [`GateKind::SeqAnd`]. So a gate
/// with k children costs exactly one clone and k - 1 calls to `op`. The
/// accumulator is passed by value, so a fold step does not need a clone.
///
/// There are no neutral elements: a gate with no children has no value and
/// [`evaluate`] reports [`ModelError::EmptyGate`] instead of guessing.
///
/// Implementations should be pure (same inputs, same output), so that
/// evaluation stays deterministic.
///
/// [`evaluate`] calls the `*_kept` variants ([`or_kept`](Self::or_kept),
/// [`and_kept`](Self::and_kept), [`seq_and_kept`](Self::seq_and_kept)),
/// which also say which input the result came from, so the evaluation can
/// record a [`Derivation`] per node. Their default bodies call the plain
/// method once and answer [`Kept::Both`] ("all inputs contributed"), which
/// is always truthful. Selection domains (max, min) should override them to
/// name the deciding child.
///
/// ```
/// use aegis_model::{evaluate, AttackTree, AttributeDomain, GateKind, Node, NodeId};
///
/// /// Minimum attacker cost: OR = cheapest child, AND and SAND = sum.
/// struct MinCost;
///
/// impl AttributeDomain for MinCost {
///     type Value = u32;
///     fn or(&self, acc: u32, next: &u32) -> u32 {
///         acc.min(*next)
///     }
///     fn and(&self, acc: u32, next: &u32) -> u32 {
///         acc.saturating_add(*next)
///     }
/// }
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
/// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
/// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
/// let eval = evaluate(&tree, &MinCost, |i| Some(if i == a { 3 } else { 4 }))?;
/// assert_eq!(*eval.root_value(), 7);
/// # Ok(())
/// # }
/// ```
pub trait AttributeDomain {
    /// The value attached to every node.
    type Value: Clone;

    /// Combines the accumulated value of an OR gate with its next child.
    fn or(&self, acc: Self::Value, next: &Self::Value) -> Self::Value;

    /// Combines the accumulated value of an AND gate with its next child.
    fn and(&self, acc: Self::Value, next: &Self::Value) -> Self::Value;

    /// Combines the accumulated value of a sequential AND gate with its next
    /// child. Defaults to [`and`](Self::and). Called in stored child order,
    /// so order-sensitive domains can rely on it.
    fn seq_and(&self, acc: Self::Value, next: &Self::Value) -> Self::Value {
        self.and(acc, next)
    }

    /// [`or`](Self::or) plus which input the result came from.
    ///
    /// Contract: the value MUST equal what `or(acc, next)` returns. Return
    /// [`Kept::Acc`] only if the result is `acc` unchanged, [`Kept::Next`]
    /// only if it is `next`'s value alone, otherwise [`Kept::Both`]. The
    /// default calls `or` exactly once and returns `Both`.
    ///
    /// Time and space: one call of `or`, plus O(1).
    ///
    /// ```
    /// use aegis_model::{AttributeDomain, Kept, MaxMin};
    ///
    /// /// Only the plain methods: the default `*_kept` answers `Both`.
    /// struct Sum;
    /// impl AttributeDomain for Sum {
    ///     type Value = u32;
    ///     fn or(&self, acc: u32, next: &u32) -> u32 { acc + next }
    ///     fn and(&self, acc: u32, next: &u32) -> u32 { acc + next }
    /// }
    ///
    /// assert_eq!(Sum.or_kept(1, &2), (3, Kept::Both));
    /// assert_eq!(MaxMin::<u8>::new().or_kept(1, &2), (2, Kept::Next));
    /// assert_eq!(MaxMin::<u8>::new().or_kept(2, &2), (2, Kept::Acc));
    /// ```
    fn or_kept(&self, acc: Self::Value, next: &Self::Value) -> (Self::Value, Kept) {
        (self.or(acc, next), Kept::Both)
    }

    /// [`and`](Self::and) plus which input the result came from. Same
    /// contract as [`or_kept`](Self::or_kept); the default calls `and`
    /// exactly once and returns [`Kept::Both`].
    ///
    /// Time and space: one call of `and`, plus O(1).
    ///
    /// ```
    /// use aegis_model::{AttributeDomain, Kept, MaxMin, MinTime};
    ///
    /// assert_eq!(MaxMin::<u8>::new().and_kept(3, &1), (1, Kept::Next));
    /// assert_eq!(MinTime.and_kept(5, &2), (5, Kept::Acc)); // parallel: max
    /// ```
    fn and_kept(&self, acc: Self::Value, next: &Self::Value) -> (Self::Value, Kept) {
        (self.and(acc, next), Kept::Both)
    }

    /// [`seq_and`](Self::seq_and) plus which input the result came from.
    /// Same contract as [`or_kept`](Self::or_kept); the default calls
    /// `seq_and` exactly once and returns [`Kept::Both`]. It does not
    /// delegate to [`and_kept`](Self::and_kept), because `seq_and` may be
    /// overridden independently of `and`.
    ///
    /// Time and space: one call of `seq_and`, plus O(1).
    ///
    /// ```
    /// use aegis_model::{AttributeDomain, Kept, MaxMin, MinTime};
    ///
    /// assert_eq!(MinTime.seq_and_kept(5, &2), (7, Kept::Both)); // sum
    /// assert_eq!(MaxMin::<u8>::new().seq_and_kept(3, &1), (1, Kept::Next));
    /// ```
    fn seq_and_kept(&self, acc: Self::Value, next: &Self::Value) -> (Self::Value, Kept) {
        (self.seq_and(acc, next), Kept::Both)
    }
}

/// Which inputs of one fold step `op(acc, &next)` the result came from.
/// Returned by the `*_kept` methods of [`AttributeDomain`].
///
/// One byte, `Copy`. Building or matching one is O(1).
///
/// ```
/// use aegis_model::{AttributeDomain, Kept, MinTime};
///
/// assert_eq!(MinTime.or_kept(4, &2), (2, Kept::Next)); // fastest alternative
/// assert_eq!(MinTime.or_kept(2, &4), (2, Kept::Acc));
/// assert_eq!(MinTime.seq_and_kept(2, &4), (6, Kept::Both));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kept {
    /// The result is the accumulator, unchanged: earlier children decided.
    Acc,
    /// The result is the next child's value alone: this child decided.
    Next,
    /// The result combines both inputs (or the domain does not say).
    Both,
}

/// How one node's value was produced, recorded by [`evaluate`] for every
/// node. Read it with [`Evaluation::derivation`], or richer with
/// [`Evaluation::explain`].
///
/// A gate folds its children `c1, ..., ck` in stored order. The record
/// starts as `Selected { child: c1 }` (the first value is cloned), then each
/// fold step updates it from the domain's [`Kept`] answer: `Acc` keeps it,
/// `Next` sets `Selected { child: ci }`, `Both` sets `Combined`. A later
/// `Next` after `Combined` returns to `Selected`, because the value was
/// replaced in full. On ties the built-in domains keep the accumulator, so
/// the earlier child is named.
///
/// At most 8 bytes, `Copy`; building or matching one is O(1).
///
/// ```
/// use aegis_model::{evaluate, AttackTree, Derivation, GateKind, MaxMin, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
/// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == b { 3 } else { 1 }))?;
/// assert_eq!(
///     eval.derivation(tree.root()),
///     Some(Derivation::Selected { rule: GateKind::Or, child: b })
/// );
/// assert_eq!(eval.derivation(a), Some(Derivation::Leaf));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Derivation {
    /// A step: the value came from the caller's leaf closure.
    Leaf,
    /// A gate whose value is exactly `child`'s value, chosen by `rule`
    /// (for example OR = most feasible child).
    Selected {
        /// The gate kind whose rule chose the child.
        rule: GateKind,
        /// The deciding child.
        child: NodeIdx,
    },
    /// A gate whose value combines several children (for example SAND =
    /// sum of times), or whose domain does not name a deciding child.
    Combined {
        /// The gate kind whose rule combined the children.
        rule: GateKind,
    },
}

impl Derivation {
    /// The gate kind, or `None` for a leaf. Time O(1).
    const fn rule(self) -> Option<GateKind> {
        match self {
            Derivation::Leaf => None,
            Derivation::Selected { rule, .. } | Derivation::Combined { rule } => Some(rule),
        }
    }

    /// The deciding child of a `Selected` gate. Time O(1).
    const fn decisive(self) -> Option<NodeIdx> {
        match self {
            Derivation::Selected { child, .. } => Some(child),
            Derivation::Leaf | Derivation::Combined { .. } => None,
        }
    }
}

/// The value of every node of one tree, from [`evaluate`].
///
/// Derived data (rule D10): it is kept apart from the [`AttackTree`] and can
/// be dropped and recomputed at any time. Values are stored in arena order,
/// so lookups by [`NodeIdx`] are O(1). Only meaningful for the tree it was
/// computed from.
///
/// Next to the values it holds the explain-why trace: one [`Derivation`]
/// per node in a separate array (struct-of-arrays, rule D4), read with
/// [`derivation`](Self::derivation), [`explain`](Self::explain), and
/// [`why_path`](Self::why_path). Two equal evaluations have equal traces.
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let s = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s"))?;
/// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(2))?;
/// assert_eq!(eval.get(s), Some(&2));
/// assert_eq!(eval.len(), 2);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evaluation<V> {
    /// Arena position to value. Every slot is `Some` except the root's,
    /// whose value was moved into `root_value` (so [`root_value`] can return
    /// `&V` without a panic path or a clone).
    ///
    /// [`root_value`]: Evaluation::root_value
    values: Vec<Option<V>>,
    /// Arena position to how its value was produced. Same length as
    /// `values`; kept apart so trace reads do not touch the values.
    derivations: Vec<Derivation>,
    /// Index of the tree root.
    root: NodeIdx,
    /// The root's value.
    root_value: V,
}

impl<V> Evaluation<V> {
    /// Returns the value of `idx`, or `None` if `idx` is not a node of the
    /// evaluated tree.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "root"))?;
    /// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let eval = evaluate(&tree, &MinTime, |i| Some(if i == a { 5 } else { 7 }))?;
    /// assert_eq!(eval.get(a), Some(&5));
    /// assert_eq!(eval.get(tree.root()), Some(&12));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn get(&self, idx: NodeIdx) -> Option<&V> {
        if idx == self.root {
            return Some(&self.root_value);
        }
        self.values.get(idx.index()).and_then(Option::as_ref)
    }

    /// Returns the value of the tree root.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let mut next = [3_u8, 1].into_iter();
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| next.next())?;
    /// assert_eq!(*eval.root_value(), 1);
    /// assert_eq!(eval.get(tree.root()), Some(eval.root_value()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn root_value(&self) -> &V {
        &self.root_value
    }

    /// Returns the number of node values, which equals the node count of
    /// the evaluated tree.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MinTime, |_| Some(1))?;
    /// assert_eq!(eval.len(), tree.len());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` if there are no values. A tree always has a root, so
    /// this is always `false`; it exists to pair with [`len`](Self::len).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// assert!(!evaluate(&tree, &MinTime, |_| Some(1))?.is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over `(index, value)` for every node, in arena order (the
    /// same order as [`AttackTree::iter`]), which is deterministic.
    ///
    /// Time O(1) per item, O(V) total; space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let mut next = [1_u8, 3].into_iter();
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| next.next())?;
    /// let values: Vec<u8> = eval.iter().map(|(_, v)| *v).collect();
    /// assert_eq!(values, [3, 3, 1]); // root, a, b (leaves run children-first)
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = (NodeIdx, &V)> {
        self.values.iter().enumerate().filter_map(|(i, slot)| {
            // INVARIANT: `values` has one slot per node of a tree, and every
            // arena position of a tree fits in a `NodeIdx`, so this never
            // drops an item.
            let idx = NodeIdx::try_from_usize(i)?;
            let value = if idx == self.root {
                Some(&self.root_value)
            } else {
                slot.as_ref()
            };
            value.map(|v| (idx, v))
        })
    }

    /// Returns how the value of `idx` was produced, or `None` if `idx` is
    /// not a node of the evaluated tree.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, Derivation, GateKind, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "root"))?;
    /// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let eval = evaluate(&tree, &MinTime, |_| Some(2))?;
    /// // SAND sums the times: no single child decides.
    /// assert_eq!(
    ///     eval.derivation(tree.root()),
    ///     Some(Derivation::Combined { rule: GateKind::SeqAnd })
    /// );
    /// assert_eq!(eval.derivation(a), Some(Derivation::Leaf));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn derivation(&self, idx: NodeIdx) -> Option<Derivation> {
        self.derivations.get(idx.index()).copied()
    }

    /// Explains the value of `idx`: its value, the gate rule, every input
    /// child with its value, and the deciding child.
    ///
    /// `tree` must be the tree this evaluation was computed from; it is
    /// borrowed so the child list is not copied (rule R1). Returns `None`
    /// if `idx` is outside the evaluation, or if `tree` visibly does not
    /// match it: a different node count, a step recorded as a gate, a gate
    /// recorded as a leaf, or a gate kind different from the recorded rule.
    /// That catches most uses with a different or edited tree; it cannot
    /// catch an edit that keeps the node count and the kinds (a new link),
    /// so recompute after every edit.
    ///
    /// Time O(1), space O(1), no allocation.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == b { 3 } else { 1 }))?;
    ///
    /// let why = eval.explain(&tree, tree.root()).ok_or(aegis_model::ModelError::UnknownNode(tree.root()))?;
    /// assert_eq!(*why.value(), 3);
    /// assert_eq!(why.rule(), Some(GateKind::Or));
    /// assert_eq!(why.decisive(), Some(b)); // OR takes the most feasible child
    /// assert_eq!(why.inputs().collect::<Vec<_>>(), [(a, &1), (b, &3)]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn explain<'a>(&'a self, tree: &'a AttackTree, idx: NodeIdx) -> Option<Explanation<'a, V>> {
        if tree.len() != self.len() {
            return None;
        }
        let derivation = self.derivation(idx)?;
        let value = self.get(idx)?;
        let children = match (tree.get(idx)?, derivation.rule()) {
            (Node::Step { .. }, None) => &[][..],
            (Node::Gate { kind, children, .. }, Some(rule)) if *kind == rule => children,
            _ => return None,
        };
        Some(Explanation {
            eval: self,
            node: idx,
            value,
            derivation,
            children,
        })
    }

    /// Follows the deciding children from `idx` down: yields `idx`, then
    /// the `Selected` child of each node, and stops after a
    /// [`Derivation::Leaf`] or a [`Derivation::Combined`] node (which is
    /// yielded last). For a selection domain such as [`MaxMin`] this is the
    /// path that dominates the result ("Feasibility High comes from P3").
    /// Every item has the same value as `idx` while the path is made of
    /// `Selected` steps. Empty if `idx` is outside the evaluation.
    ///
    /// A shared step is reported exactly: its value is the same for every
    /// parent, so the path is the same whichever parent is followed.
    ///
    /// Lazy: time O(1) per item, O(depth) total; space O(1), no allocation,
    /// no recursion. Defense in depth: it yields at most [`len`](Self::len)
    /// items. Does not need the tree.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let and = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "and"))?;
    /// let a = tree.add_child(and, NodeId::new_v7(), Node::step("a"))?;
    /// let b = tree.add_child(and, NodeId::new_v7(), Node::step("b"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("c"))?;
    /// let level = |i| Some(if i == a { 4_u8 } else if i == b { 3 } else { 1 });
    /// let eval = evaluate(&tree, &MaxMin::new(), level)?;
    /// // max(min(4, 3), 1) = 3, decided by `and`, which got it from `b`.
    /// assert_eq!(eval.why_path(tree.root()).collect::<Vec<_>>(), [tree.root(), and, b]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn why_path(&self, idx: NodeIdx) -> impl Iterator<Item = NodeIdx> + '_ {
        let start = self.derivation(idx).map(|_| idx);
        std::iter::successors(start, move |&cur| self.derivation(cur)?.decisive()).take(self.len())
    }
}

/// The explanation of one node's value, from [`Evaluation::explain`].
///
/// Borrows the evaluation and the tree, so it is `Copy` and building it
/// costs O(1). Holds no text: wording and labels belong to the UI and
/// report layers.
///
/// ```
/// use aegis_model::{evaluate, AttackTree, Derivation, GateKind, MinTime, ModelError, Node, NodeId};
///
/// # fn main() -> Result<(), ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "root"))?;
/// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
/// let eval = evaluate(&tree, &MinTime, |i| Some(if i == a { 2 } else { 5 }))?;
///
/// let why = eval.explain(&tree, tree.root()).ok_or(ModelError::UnknownNode(tree.root()))?;
/// assert_eq!(why.node(), tree.root());
/// assert_eq!(*why.value(), 7);
/// assert_eq!(why.derivation(), Derivation::Combined { rule: GateKind::SeqAnd });
/// assert_eq!(why.decisive(), None); // SAND sums all children
/// assert_eq!(why.inputs().collect::<Vec<_>>(), [(a, &2), (b, &5)]);
/// # Ok(())
/// # }
/// ```
pub struct Explanation<'a, V> {
    /// The evaluation, for the children's values.
    eval: &'a Evaluation<V>,
    /// The explained node.
    node: NodeIdx,
    /// Its value.
    value: &'a V,
    /// How the value was produced.
    derivation: Derivation,
    /// The gate's children in fold order; empty for a step.
    children: &'a [NodeIdx],
}

impl<V> Clone for Explanation<'_, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V> Copy for Explanation<'_, V> {}

impl<V: fmt::Debug> fmt::Debug for Explanation<'_, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Explanation")
            .field("node", &self.node)
            .field("value", self.value)
            .field("derivation", &self.derivation)
            .field("children", &self.children)
            .finish_non_exhaustive()
    }
}

impl<'a, V> Explanation<'a, V> {
    /// The explained node. Time O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MinTime, |_| Some(1))?;
    /// assert_eq!(eval.explain(&tree, tree.root()).map(|e| e.node()), Some(tree.root()));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn node(&self) -> NodeIdx {
        self.node
    }

    /// The node's value, the same as [`Evaluation::get`]. Time O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MinTime, |_| Some(9))?;
    /// assert_eq!(eval.explain(&tree, tree.root()).map(|e| *e.value()), Some(9));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn value(&self) -> &'a V {
        self.value
    }

    /// How the value was produced, the same as [`Evaluation::derivation`].
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, Derivation, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let eval = evaluate(&tree, &MinTime, |_| Some(1))?;
    /// assert_eq!(eval.explain(&tree, tree.root()).map(|e| e.derivation()), Some(Derivation::Leaf));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn derivation(&self) -> Derivation {
        self.derivation
    }

    /// The gate kind whose rule produced the value, or `None` for a step.
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// let s = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s"))?;
    /// let eval = evaluate(&tree, &MinTime, |_| Some(1))?;
    /// assert_eq!(eval.explain(&tree, tree.root()).and_then(|e| e.rule()), Some(GateKind::And));
    /// assert_eq!(eval.explain(&tree, s).and_then(|e| e.rule()), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn rule(&self) -> Option<GateKind> {
        self.derivation.rule()
    }

    /// The child whose value the gate took, or `None` for a step or a
    /// [`Derivation::Combined`] gate. Time O(1).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MinTime, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let fast = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("fast"))?;
    /// tree.add_child(tree.root(), NodeId::new_v7(), Node::step("slow"))?;
    /// let eval = evaluate(&tree, &MinTime, |i| Some(if i == fast { 1 } else { 8 }))?;
    /// assert_eq!(eval.explain(&tree, tree.root()).and_then(|e| e.decisive()), Some(fast));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn decisive(&self) -> Option<NodeIdx> {
        self.derivation.decisive()
    }

    /// The gate's children in fold (stored) order, each with its value.
    /// Empty for a step.
    ///
    /// Lazy: time O(1) per item, O(k) total for k children; space O(1), no
    /// allocation.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// let a = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// let b = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("b"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| Some(if i == a { 2 } else { 4 }))?;
    /// let inputs: Vec<_> = eval.explain(&tree, tree.root()).into_iter().flat_map(|e| e.inputs()).collect();
    /// assert_eq!(inputs, [(a, &2), (b, &4)]);
    /// assert_eq!(eval.explain(&tree, a).map(|e| e.inputs().count()), Some(0));
    /// # Ok(())
    /// # }
    /// ```
    pub fn inputs(&self) -> impl Iterator<Item = (NodeIdx, &'a V)> + use<'a, V> {
        let eval = self.eval;
        // INVARIANT: `explain` checked that the tree has as many nodes as
        // the evaluation, so every child index has a value and nothing is
        // dropped here.
        self.children
            .iter()
            .filter_map(move |&c| eval.get(c).map(|v| (c, v)))
    }
}

/// Computes the value of every node of `tree` in `domain`.
///
/// `leaf` gives the value of each step and is called exactly once per step.
/// Each gate folds its children's values in stored order as described on
/// [`AttributeDomain`]. Shared nodes (added with [`AttackTree::link`]) are
/// evaluated once and their value is reused by every parent, so a DAG with
/// exponentially many root-to-leaf paths still costs O(V + E).
///
/// Order: nodes are visited in the reverse of
/// [`AttackTree::topological_order`] (Kahn's algorithm, parents first), so
/// every child is done before its parents. The order is fixed for a given
/// tree, so the result and the first error reported are deterministic.
///
/// Limitation: for domains that are not idempotent (sum of costs,
/// probability), a step shared by several paths contributes once per path
/// that reaches it, because each gate combines its children's values
/// independently. See ADR 0005.
///
/// Trace: every node also gets a [`Derivation`] in the same pass. Each
/// fold step calls the domain's `*_kept` method (see [`AttributeDomain`])
/// and updates the gate's record from the answer, in O(1). See
/// [`Evaluation::explain`] and [`Evaluation::why_path`].
///
/// Time O(V + E) plus V calls of `leaf` (steps only) and E - G calls of the
/// domain (G = gates). Extra space O(V) for the order, the values, and the
/// derivations (8 bytes per node). No recursion, so deep trees cannot
/// overflow the stack.
///
/// # Errors
///
/// Stops at the first error in evaluation order:
/// - [`ModelError::Cycle`] if the graph has a cycle (from
///   [`AttackTree::topological_order`]; cannot happen for a tree built
///   through the public API);
/// - [`ModelError::MissingLeafValue`] with the step's ID if `leaf` returns
///   `None`;
/// - [`ModelError::EmptyGate`] with the gate's ID if a gate has no children.
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, ModelError, Node, NodeId};
///
/// # fn main() -> Result<(), ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let and = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "and"))?;
/// let a = tree.add_child(and, NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(and, NodeId::new_v7(), Node::step("b"))?;
/// let c = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("c"))?;
/// tree.link(and, c)?; // `c` is shared by the root and `and`
///
/// let levels = |i| [(a, 4_u8), (b, 2), (c, 3)].into_iter().find(|(n, _)| *n == i).map(|(_, v)| v);
/// let eval = evaluate(&tree, &MaxMin::<u8>::new(), levels)?;
/// assert_eq!(eval.get(and), Some(&2)); // min(4, 2, 3)
/// assert_eq!(*eval.root_value(), 3); // max(2, 3)
///
/// let missing = evaluate(&tree, &MaxMin::<u8>::new(), |_| None);
/// assert!(matches!(missing, Err(ModelError::MissingLeafValue(_))));
/// # Ok(())
/// # }
/// ```
pub fn evaluate<D, F>(
    tree: &AttackTree,
    domain: &D,
    mut leaf: F,
) -> Result<Evaluation<D::Value>, ModelError>
where
    D: AttributeDomain,
    F: FnMut(NodeIdx) -> Option<D::Value>,
{
    // Kahn's algorithm gives parents first; walking it backwards gives a
    // children-first order, the classic memoized bottom-up evaluation of an
    // attribute domain over a DAG (Mauw and Oostdijk 2005).
    let order = tree.topological_order()?;
    let mut values: Vec<Option<D::Value>> = Vec::with_capacity(tree.len());
    values.resize_with(tree.len(), || None);
    // Every slot is overwritten below, because the order lists every node.
    let mut derivations: Vec<Derivation> = Vec::with_capacity(tree.len());
    derivations.resize(tree.len(), Derivation::Leaf);

    for &idx in order.iter().rev() {
        let (value, how) = match tree.get(idx) {
            Some(Node::Step { .. }) => match leaf(idx) {
                Some(v) => (v, Derivation::Leaf),
                None => return Err(ModelError::MissingLeafValue(id_of(tree, idx)?)),
            },
            Some(Node::Gate { kind, children, .. }) => {
                let rule = *kind;
                let folded = match rule {
                    GateKind::Or => fold(&values, children, rule, |a, v| domain.or_kept(a, v)),
                    GateKind::And => fold(&values, children, rule, |a, v| domain.and_kept(a, v)),
                    GateKind::SeqAnd => {
                        fold(&values, children, rule, |a, v| domain.seq_and_kept(a, v))
                    }
                };
                match folded {
                    Ok(Some(done)) => done,
                    Ok(None) => return Err(ModelError::EmptyGate(id_of(tree, idx)?)),
                    Err(missing) => return Err(ModelError::UnknownNode(missing)),
                }
            }
            // INVARIANT: the order lists only nodes of `tree`.
            None => return Err(ModelError::UnknownNode(idx)),
        };
        if let Some(slot) = values.get_mut(idx.index()) {
            *slot = Some(value);
        }
        if let Some(slot) = derivations.get_mut(idx.index()) {
            *slot = how;
        }
    }

    let root = tree.root();
    match values.get_mut(root.index()).and_then(Option::take) {
        Some(root_value) => Ok(Evaluation {
            values,
            derivations,
            root,
            root_value,
        }),
        // INVARIANT: the order contains every node, including the root, so
        // its slot was filled above.
        None => Err(ModelError::UnknownNode(root)),
    }
}

/// Left fold of the children's memoized values: a clone of the first, then
/// `combine(acc, &v)` for the rest, in stored order. Also runs the trace
/// state machine (see [`Derivation`]): start at `Selected(c1)`; per step,
/// `Acc` keeps the record, `Next` selects that child, `Both` sets
/// `Combined`.
///
/// Returns `Ok(None)` for no children, and `Err(child)` if a child has no
/// value yet. INVARIANT: the latter cannot happen, because the caller visits
/// nodes in reverse topological order, so every child is done before its
/// parent; it is an error instead of a panic (rule Q3).
///
/// Time O(k) for k children, plus one clone and k - 1 `combine` calls.
fn fold<V: Clone>(
    values: &[Option<V>],
    children: &[NodeIdx],
    rule: GateKind,
    mut combine: impl FnMut(V, &V) -> (V, Kept),
) -> Result<Option<(V, Derivation)>, NodeIdx> {
    let value = |c: NodeIdx| values.get(c.index()).and_then(Option::as_ref).ok_or(c);
    let mut rest = children.iter();
    let Some(&first) = rest.next() else {
        return Ok(None);
    };
    let mut acc = value(first)?.clone();
    let mut how = Derivation::Selected { rule, child: first };
    for &c in rest {
        let (next, kept) = combine(acc, value(c)?);
        acc = next;
        match kept {
            Kept::Acc => {}
            Kept::Next => how = Derivation::Selected { rule, child: c },
            Kept::Both => how = Derivation::Combined { rule },
        }
    }
    Ok(Some((acc, how)))
}

/// The public ID of `idx`, for error payloads. Time O(1).
fn id_of(tree: &AttackTree, idx: NodeIdx) -> Result<crate::id::NodeId, ModelError> {
    tree.id_of(idx).ok_or(ModelError::UnknownNode(idx))
}

/// Ordered levels, for example feasibility: OR = maximum (the most feasible
/// child wins), AND and SAND = minimum (the hardest child limits the gate).
///
/// Works for any `T: Ord + Clone`, such as `u8` or an ordered enum. On ties
/// the earlier child's value is kept. A zero-sized marker: building one costs
/// nothing. Each combine is O(1) comparisons plus at most one clone of `T`.
/// Every gate's trace is [`Derivation::Selected`]: it names the deciding
/// child (the earlier one on a tie).
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// enum Feasibility { VeryLow, Low, Medium, High }
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let and = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "both"))?;
/// let a = tree.add_child(and, NodeId::new_v7(), Node::step("a"))?;
/// tree.add_child(and, NodeId::new_v7(), Node::step("b"))?;
/// let c = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("c"))?;
///
/// let level = |i| Some(if i == a { Feasibility::High } else if i == c { Feasibility::Low } else { Feasibility::Medium });
/// let eval = evaluate(&tree, &MaxMin::new(), level)?;
/// assert_eq!(eval.get(and), Some(&Feasibility::Medium));
/// assert_eq!(*eval.root_value(), Feasibility::Medium);
/// assert!(Feasibility::VeryLow < Feasibility::Low);
/// # Ok(())
/// # }
/// ```
pub struct MaxMin<T>(PhantomData<fn() -> T>);

impl<T> MaxMin<T> {
    /// Creates the domain. Time O(1), space O(1) (zero-sized).
    ///
    /// ```
    /// use aegis_model::{AttributeDomain, MaxMin};
    ///
    /// let d = MaxMin::<u8>::new();
    /// assert_eq!(d.or(1, &3), 3);
    /// assert_eq!(d.and(1, &3), 1);
    /// assert_eq!(d.seq_and(1, &3), 1);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        MaxMin(PhantomData)
    }
}

impl<T> Default for MaxMin<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for MaxMin<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for MaxMin<T> {}

impl<T> fmt::Debug for MaxMin<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MaxMin")
    }
}

impl<T: Ord + Clone> AttributeDomain for MaxMin<T> {
    type Value = T;

    /// Maximum; keeps `acc` on a tie. Time O(1) plus at most one clone.
    fn or(&self, acc: T, next: &T) -> T {
        self.or_kept(acc, next).0
    }

    /// Minimum; keeps `acc` on a tie. Time O(1) plus at most one clone.
    fn and(&self, acc: T, next: &T) -> T {
        self.and_kept(acc, next).0
    }

    /// Maximum: [`Kept::Next`] if `next` is strictly greater, else
    /// [`Kept::Acc`] (a tie keeps the earlier child). The plain `or` uses
    /// this comparison, so the two cannot disagree. Time O(1) plus at most
    /// one clone.
    fn or_kept(&self, acc: T, next: &T) -> (T, Kept) {
        if *next > acc {
            (next.clone(), Kept::Next)
        } else {
            (acc, Kept::Acc)
        }
    }

    /// Minimum: [`Kept::Next`] if `next` is strictly smaller, else
    /// [`Kept::Acc`] (a tie keeps the earlier child). Time O(1) plus at
    /// most one clone.
    fn and_kept(&self, acc: T, next: &T) -> (T, Kept) {
        if *next < acc {
            (next.clone(), Kept::Next)
        } else {
            (acc, Kept::Acc)
        }
    }

    /// Same as [`and_kept`](Self::and_kept), matching the default
    /// `seq_and` = `and`. Time O(1) plus at most one clone.
    fn seq_and_kept(&self, acc: T, next: &T) -> (T, Kept) {
        self.and_kept(acc, next)
    }
}

/// Minimum attack time in abstract units (`u64`): OR = minimum (the fastest
/// alternative), AND = maximum (children run in parallel), SAND = sum
/// (children run one after another). The sum saturates at `u64::MAX`
/// instead of overflowing. Each combine is O(1). OR and AND gates trace
/// the deciding child ([`Derivation::Selected`], the earlier one on a tie);
/// a SAND gate with two or more children is [`Derivation::Combined`].
///
/// ```
/// use aegis_model::{evaluate, AttackTree, GateKind, MinTime, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let par = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "par"))?;
/// let seq = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "seq"))?;
/// let a = tree.add_child(par, NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(par, NodeId::new_v7(), Node::step("b"))?;
/// tree.link(seq, a)?;
/// tree.link(seq, b)?;
/// let eval = evaluate(&tree, &MinTime, |i| Some(if i == a { 2 } else { 3 }))?;
/// assert_eq!(eval.get(par), Some(&3)); // max(2, 3)
/// assert_eq!(eval.get(seq), Some(&5)); // 2 + 3
/// assert_eq!(*eval.root_value(), 3); // min(3, 5)
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MinTime;

impl AttributeDomain for MinTime {
    type Value = u64;

    /// Minimum. Time O(1).
    fn or(&self, acc: u64, next: &u64) -> u64 {
        self.or_kept(acc, next).0
    }

    /// Maximum (parallel). Time O(1).
    fn and(&self, acc: u64, next: &u64) -> u64 {
        self.and_kept(acc, next).0
    }

    /// Saturating sum (sequential). Time O(1).
    fn seq_and(&self, acc: u64, next: &u64) -> u64 {
        self.seq_and_kept(acc, next).0
    }

    /// Minimum: [`Kept::Next`] if `next` is strictly smaller, else
    /// [`Kept::Acc`] (a tie keeps the earlier child). Time O(1).
    fn or_kept(&self, acc: u64, next: &u64) -> (u64, Kept) {
        if *next < acc {
            (*next, Kept::Next)
        } else {
            (acc, Kept::Acc)
        }
    }

    /// Maximum: [`Kept::Next`] if `next` is strictly greater, else
    /// [`Kept::Acc`] (a tie keeps the earlier child). Time O(1).
    fn and_kept(&self, acc: u64, next: &u64) -> (u64, Kept) {
        if *next > acc {
            (*next, Kept::Next)
        } else {
            (acc, Kept::Acc)
        }
    }

    /// Saturating sum, always [`Kept::Both`]: every child adds its time.
    /// Time O(1).
    fn seq_and_kept(&self, acc: u64, next: &u64) -> (u64, Kept) {
        (acc.saturating_add(*next), Kept::Both)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_min_combines() {
        let d = MaxMin::<u8>::new();
        assert_eq!(d.or(1, &3), 3);
        assert_eq!(d.or(3, &1), 3);
        assert_eq!(d.and(1, &3), 1);
        assert_eq!(d.and(3, &1), 1);
        assert_eq!(d.seq_and(3, &1), 1);
        assert_eq!(MaxMin::<u8>::default().or(2, &2), 2);
        assert_eq!(format!("{:?}", MaxMin::<u8>::new()), "MaxMin");
    }

    /// Ties keep the accumulator: visible with a type whose `Ord` ignores
    /// part of the value.
    #[test]
    fn max_min_keeps_first_on_tie() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Tagged(u8, &'static str);
        impl PartialOrd for Tagged {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for Tagged {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.0.cmp(&other.0)
            }
        }
        let d = MaxMin::<Tagged>::new();
        assert_eq!(d.or(Tagged(1, "a"), &Tagged(1, "b")).1, "a");
        assert_eq!(d.and(Tagged(1, "a"), &Tagged(1, "b")).1, "a");
    }

    #[test]
    fn min_time_combines() {
        assert_eq!(MinTime.or(4, &2), 2);
        assert_eq!(MinTime.and(4, &2), 4);
        assert_eq!(MinTime.seq_and(4, &2), 6);
        assert_eq!(MinTime.seq_and(u64::MAX, &1), u64::MAX);
    }

    #[test]
    fn fold_reports_missing_child() {
        let values: Vec<Option<u8>> = vec![Some(1), None];
        let children = [NodeIdx::try_from_usize(0), NodeIdx::try_from_usize(1)];
        let children: Vec<NodeIdx> = children.into_iter().flatten().collect();
        let max = |a: u8, b: &u8| (a.max(*b), Kept::Both);
        let got = fold(&values, &children, GateKind::Or, max);
        assert_eq!(got, Err(children[1]));
        assert_eq!(fold(&values, &[], GateKind::Or, max), Ok(None));
        let one = fold(&values, &children[..1], GateKind::And, max);
        let want = Derivation::Selected {
            rule: GateKind::And,
            child: children[0],
        };
        assert_eq!(one, Ok(Some((1, want))));
    }

    /// AC6: the `*_kept` value equals the plain method, and the tag says
    /// which input the result is.
    #[test]
    fn kept_matches_plain() {
        let d = MaxMin::<u8>::new();
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                let tag = |pick_next: bool| if pick_next { Kept::Next } else { Kept::Acc };
                assert_eq!(d.or_kept(a, &b), (d.or(a, &b), tag(b > a)), "or {a} {b}");
                assert_eq!(d.and_kept(a, &b), (d.and(a, &b), tag(b < a)), "and {a} {b}");
                assert_eq!(d.seq_and_kept(a, &b), (d.seq_and(a, &b), tag(b < a)));
                assert_eq!(d.or(a, &b), a.max(b));
                assert_eq!(d.and(a, &b), a.min(b));
            }
        }

        let samples = [0, 1, 2, 7, 1_000, u64::MAX / 2, u64::MAX - 1, u64::MAX];
        for a in samples {
            for b in samples {
                let tag = |pick_next: bool| if pick_next { Kept::Next } else { Kept::Acc };
                assert_eq!(MinTime.or_kept(a, &b), (MinTime.or(a, &b), tag(b < a)));
                assert_eq!(MinTime.and_kept(a, &b), (MinTime.and(a, &b), tag(b > a)));
                let sum = MinTime.seq_and_kept(a, &b);
                assert_eq!(sum, (MinTime.seq_and(a, &b), Kept::Both));
                assert_eq!(MinTime.or(a, &b), a.min(b));
                assert_eq!(MinTime.and(a, &b), a.max(b));
                let wide = u128::from(a) + u128::from(b);
                assert_eq!(sum.0, u64::try_from(wide).unwrap_or(u64::MAX));
            }
        }
    }

    /// The tag on a tie names the accumulator, also when `Ord` ignores part
    /// of the value (so a wrong tag would be visible in the value too).
    #[test]
    fn kept_ties_keep_acc() {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        struct Level(u8);
        let d = MaxMin::<Level>::new();
        assert_eq!(d.or_kept(Level(2), &Level(2)), (Level(2), Kept::Acc));
        assert_eq!(d.and_kept(Level(2), &Level(2)), (Level(2), Kept::Acc));
        assert_eq!(MinTime.or_kept(3, &3), (3, Kept::Acc));
        assert_eq!(MinTime.and_kept(3, &3), (3, Kept::Acc));
    }

    /// The default `*_kept` bodies call the plain method and answer `Both`.
    #[test]
    fn default_kept_is_both() {
        struct Sum;
        impl AttributeDomain for Sum {
            type Value = u32;
            fn or(&self, acc: u32, next: &u32) -> u32 {
                acc + next
            }
            fn and(&self, acc: u32, next: &u32) -> u32 {
                acc * next
            }
        }
        assert_eq!(Sum.or_kept(2, &3), (5, Kept::Both));
        assert_eq!(Sum.and_kept(2, &3), (6, Kept::Both));
        assert_eq!(Sum.seq_and_kept(2, &3), (6, Kept::Both));
    }

    /// AC12: one record per node stays small.
    #[test]
    fn derivation_size() {
        assert!(std::mem::size_of::<Derivation>() <= 8);
        assert_eq!(std::mem::size_of::<Kept>(), 1);
    }

    #[test]
    fn derivation_helpers() {
        let c = NodeIdx::try_from_usize(3);
        assert!(c.is_some());
        let Some(c) = c else { return };
        let sel = Derivation::Selected {
            rule: GateKind::Or,
            child: c,
        };
        let comb = Derivation::Combined {
            rule: GateKind::SeqAnd,
        };
        assert_eq!(sel.rule(), Some(GateKind::Or));
        assert_eq!(sel.decisive(), Some(c));
        assert_eq!(comb.rule(), Some(GateKind::SeqAnd));
        assert_eq!(comb.decisive(), None);
        assert_eq!(Derivation::Leaf.rule(), None);
        assert_eq!(Derivation::Leaf.decisive(), None);
    }

    #[test]
    fn explanation_debug_and_copy() -> Result<(), ModelError> {
        let mut tree = AttackTree::new(
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "root"),
        )?;
        let s = tree.add_child(tree.root(), crate::id::NodeId::new_v7(), Node::step("s"))?;
        let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(4))?;
        let why = eval.explain(&tree, tree.root());
        assert!(why.is_some());
        let Some(why) = why else { return Ok(()) };
        let copy = why;
        assert_eq!(copy.node(), why.node());
        let text = format!("{why:?}");
        assert!(text.starts_with("Explanation"), "{text}");
        assert!(text.contains("value: 4"), "{text}");
        assert!(text.contains(&format!("{s:?}")), "{text}");
        Ok(())
    }
}
