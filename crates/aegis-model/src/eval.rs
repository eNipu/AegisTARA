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

use std::collections::{BTreeMap, BinaryHeap};
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

    /// Returns a lazy iterator over the minimal leaf-step attack paths of
    /// `tree` in descending order of feasibility (easiest for the attacker
    /// first), guided by the node values in `self` (rule A4).
    ///
    /// At each [`GateKind::Or`] gate a path chooses one child branch; at each
    /// [`GateKind::And`] or [`GateKind::SeqAnd`] gate it includes all child
    /// branches. Returns `None` if `tree` visibly does not match `self` (a
    /// different node count, a step recorded as a gate, a gate recorded as a
    /// leaf, or a gate kind different from the recorded rule).
    ///
    /// Lazy: building the iterator costs O(V + E) time and O(V) space; each
    /// [`Iterator::next`] call expands only the highest-bound partial paths
    /// without materializing all $2^{\Omega(d)}$ paths in an AND/OR tree.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let and = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "and"))?;
    /// let a = tree.add_child(and, NodeId::new_v7(), Node::step("a"))?;
    /// let b = tree.add_child(and, NodeId::new_v7(), Node::step("b"))?;
    /// let c = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("c"))?;
    /// let level = |i| Some(if i == a { 5_u8 } else if i == b { 3 } else { 4 });
    /// let eval = evaluate(&tree, &MaxMin::new(), level)?;
    /// let paths: Vec<_> = eval
    ///     .attack_paths(&tree)
    ///     .ok_or(aegis_model::ModelError::UnknownNode(a))?
    ///     .collect();
    /// assert_eq!(paths.len(), 2);
    /// assert_eq!(paths[0].steps(), &[c]);
    /// assert_eq!(*paths[0].feasibility(), 4);
    /// assert_eq!(paths[1].steps(), &[a, b]);
    /// assert_eq!(*paths[1].feasibility(), 3);
    /// assert_eq!(paths[1].bottleneck(), b);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_paths<'a>(
        &'a self,
        tree: &'a AttackTree,
    ) -> Option<crate::paths::AttackPaths<'a, V>>
    where
        V: Ord + Clone,
    {
        crate::paths::AttackPaths::new(tree, self)
    }

    /// Collects up to `k` of the easiest minimal attack paths of `tree` in
    /// descending order of feasibility (rule A4).
    ///
    /// Returns `None` if `tree` visibly does not match `self` (see
    /// [`attack_paths`](Self::attack_paths)). When `k == 0`, returns
    /// `Some(Vec::new())` after validating `tree`.
    ///
    /// Time O((V + E) + k * V * (V + E) * log(k * d)) on trees where `d` is
    /// the maximum gate arity; space O(V + k * d * V).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let s1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s1"))?;
    /// let s2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s2"))?;
    /// let s3 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s3"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| {
    ///     Some(if i == s1 { 2 } else if i == s2 { 5 } else { 4 })
    /// })?;
    /// let top2 = eval.top_k_paths(&tree, 2).ok_or(aegis_model::ModelError::UnknownNode(s1))?;
    /// assert_eq!(top2.len(), 2);
    /// assert_eq!(top2[0].steps(), &[s2]);
    /// assert_eq!(top2[1].steps(), &[s3]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn top_k_paths(
        &self,
        tree: &AttackTree,
        k: usize,
    ) -> Option<Vec<crate::paths::AttackPath<V>>>
    where
        V: Ord + Clone,
    {
        Some(self.attack_paths(tree)?.take(k).collect())
    }

    /// Ranks the leaf steps of `tree` by how many of the top-`k` easiest
    /// minimal attack paths each one lies on (choke points, rule A4).
    ///
    /// Each returned [`ChokePoint`](crate::ChokePoint) counts, across the up
    /// to `k` paths [`top_k_paths`](Self::top_k_paths) yields, how many
    /// contain the step, how many have it as their bottleneck, and the
    /// feasibility of the easiest containing path. Steps that lie on none of
    /// the top-`k` paths are omitted; `k == 0` yields an empty ranking. The
    /// ranking is sorted by path coverage descending, then bottleneck count
    /// descending, then by arena position ascending, so it is deterministic
    /// (rule P4).
    ///
    /// Time O((V + E) + k * V * (V + E) * log(k * d)) to enumerate the paths
    /// (inherited from [`attack_paths`](Self::attack_paths)) plus O(k * S)
    /// counting and O(C log C) sorting, where S is the mean path length and
    /// C <= V is the number of covered steps. Space O(V + k * d * V).
    ///
    /// Returns `None` if `tree` visibly does not match `self` (see
    /// [`attack_paths`](Self::attack_paths)).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let s1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s1"))?;
    /// let s2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s2"))?;
    /// let s3 = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("s3"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |i| {
    ///     Some(if i == s1 { 2 } else if i == s2 { 5 } else { 4 })
    /// })?;
    /// let choke = eval.choke_points(&tree, 2).ok_or(aegis_model::ModelError::UnknownNode(s1))?;
    /// // Only the two easiest paths ([s2] and [s3]) are counted, so `s1` is absent.
    /// assert_eq!(choke.iter().map(|c| c.step()).collect::<Vec<_>>(), [s2, s3]);
    /// assert!(choke.iter().all(|c| c.path_count() == 1));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn choke_points(
        &self,
        tree: &AttackTree,
        k: usize,
    ) -> Option<Vec<crate::paths::ChokePoint<V>>>
    where
        V: Ord + Clone,
    {
        let mut counts = vec![0_usize; tree.len()];
        let mut bottlenecks = vec![0_usize; tree.len()];
        // Because paths are yielded in descending order of feasibility, the
        // first path containing a step is that step's easiest containing path.
        let mut easiest: Vec<Option<V>> = vec![None; tree.len()];
        for path in self.attack_paths(tree)?.take(k) {
            for &s in path.steps() {
                if let Some(slot) = counts.get_mut(s.index()) {
                    *slot += 1;
                }
                let slot = easiest.get_mut(s.index())?;
                if slot.is_none() {
                    *slot = Some(path.feasibility().clone());
                }
            }
            if let Some(slot) = bottlenecks.get_mut(path.bottleneck().index()) {
                *slot += 1;
            }
        }

        let mut out = Vec::new();
        for (idx, _, node) in tree.iter() {
            if !matches!(node, Node::Step { .. }) {
                continue;
            }
            let Some(&count) = counts.get(idx.index()) else {
                continue;
            };
            if count == 0 {
                continue;
            }
            let bottleneck_count = bottlenecks.get(idx.index()).copied().unwrap_or(0);
            let Some(easiest_path_feasibility) = easiest.get(idx.index()).cloned().flatten() else {
                continue;
            };
            out.push(crate::paths::ChokePoint {
                step: idx,
                path_count: count,
                bottleneck_count,
                easiest_path_feasibility,
            });
        }
        out.sort_by(|a, b| {
            b.path_count()
                .cmp(&a.path_count())
                .then_with(|| b.bottleneck_count().cmp(&a.bottleneck_count()))
                .then_with(|| a.step().cmp(&b.step()))
        });
        Some(out)
    }

    /// Returns a small set of leaf steps that together hits (contains at
    /// least one step of) every one of the top-`k` easiest minimal attack
    /// paths of `tree`: mitigating all of them leaves the attacker no path
    /// among those `k` best options (rule A4).
    ///
    /// The set is built with the classic greedy heuristic for set cover
    /// (V. Chvátal, "A greedy heuristic for the set-covering problem",
    /// *Mathematics of Operations Research* 4(3), 1979), applied to the
    /// dual hitting-set instance: each round picks the step that lies on
    /// the most still-unhit paths, breaking ties by the smallest arena
    /// position, so the result is deterministic (rule P4). The result is a
    /// heuristic: it is within a factor of $H(k) \le \ln k + 1$ of the
    /// smallest possible hitting set over those same paths, and it is not
    /// guaranteed to be minimum. Steps are returned in pick order (most
    /// coverage first). `k == 0` yields an empty set.
    ///
    /// Time O((V + E) + k * V * (V + E) * log(k * d)) to enumerate the paths
    /// plus O(R * U * S * log S) for the greedy rounds, where R <= k is the
    /// number of picks, U <= k the still-unhit paths per round, and S the
    /// mean path length. Space O(V + k * d * V).
    ///
    /// Returns `None` if `tree` visibly does not match `self` (see
    /// [`attack_paths`](Self::attack_paths)).
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MaxMin, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// // root = OR(AND(a, b), AND(a, c), d): `a` hits the first two paths
    /// // on its own, so the greedy picks cover all three with [a, d].
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let and1 = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "and1"))?;
    /// let a = tree.add_child(and1, NodeId::new_v7(), Node::step("a"))?;
    /// let _b = tree.add_child(and1, NodeId::new_v7(), Node::step("b"))?;
    /// let and2 = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "and2"))?;
    /// tree.link(and2, a)?;
    /// let _c = tree.add_child(and2, NodeId::new_v7(), Node::step("c"))?;
    /// let d = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("d"))?;
    /// let eval = evaluate(&tree, &MaxMin::<u8>::new(), |_| Some(3))?;
    /// let set = eval
    ///     .top_k_hitting_set(&tree, 3)
    ///     .ok_or(aegis_model::ModelError::UnknownNode(a))?;
    /// assert_eq!(set, vec![a, d]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn top_k_hitting_set(&self, tree: &AttackTree, k: usize) -> Option<Vec<NodeIdx>>
    where
        V: Ord + Clone,
    {
        let paths: Vec<Box<[NodeIdx]>> = self
            .attack_paths(tree)?
            .take(k)
            .map(|p| p.steps().to_vec().into_boxed_slice())
            .collect();
        let mut covered = vec![false; paths.len()];
        let mut chosen: Vec<NodeIdx> = Vec::new();
        loop {
            // Count, per step, how many still-unhit paths contain it. A
            // `BTreeMap` iterates in ascending `NodeIdx` order, and only a
            // strictly greater count replaces the current best, so ties keep
            // the smallest arena position.
            let mut counts: BTreeMap<NodeIdx, usize> = BTreeMap::new();
            for (pi, steps) in paths.iter().enumerate() {
                if covered.get(pi).copied().unwrap_or(true) {
                    continue;
                }
                for &s in steps {
                    *counts.entry(s).or_insert(0) += 1;
                }
            }
            let mut best: Option<(NodeIdx, usize)> = None;
            for (&step, &count) in &counts {
                if best.is_none_or(|(_, bc)| count > bc) {
                    best = Some((step, count));
                }
            }
            let Some((best_step, _)) = best else {
                break;
            };
            chosen.push(best_step);
            for (pi, steps) in paths.iter().enumerate() {
                if steps.contains(&best_step)
                    && let Some(slot) = covered.get_mut(pi)
                {
                    *slot = true;
                }
            }
        }
        Some(chosen)
    }

    /// Updates `idx`'s value and derivation in place, returning `(value_changed, any_changed)`.
    fn set_node(&mut self, idx: NodeIdx, value: V, how: Derivation) -> (bool, bool)
    where
        V: PartialEq,
    {
        let deriv_changed = match self.derivations.get_mut(idx.index()) {
            Some(slot) => {
                let changed = *slot != how;
                *slot = how;
                changed
            }
            None => false,
        };
        let val_changed = if idx == self.root {
            let changed = self.root_value != value;
            self.root_value = value;
            changed
        } else if let Some(slot) = self.values.get_mut(idx.index()) {
            let changed = slot.as_ref() != Some(&value);
            *slot = Some(value);
            changed
        } else {
            false
        };
        (val_changed, val_changed || deriv_changed)
    }

    /// Synchronizes only `nodes` (and `root_value`) from `source` into `self`
    /// in O(|nodes|) time without reallocating.
    pub(crate) fn sync_nodes(&mut self, source: &Self, nodes: &[NodeIdx])
    where
        V: Clone,
    {
        self.root = source.root;
        self.root_value = source.root_value.clone();
        for &idx in nodes {
            let i = idx.index();
            if let (Some(dst), Some(src)) = (self.values.get_mut(i), source.values.get(i)) {
                dst.clone_from(src);
            }
            if let (Some(dst), Some(src)) = (self.derivations.get_mut(i), source.derivations.get(i))
            {
                *dst = *src;
            }
        }
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
    leaf: F,
) -> Result<Evaluation<D::Value>, ModelError>
where
    D: AttributeDomain,
    F: FnMut(NodeIdx) -> Option<D::Value>,
{
    let order = tree.topological_order()?;
    evaluate_with_order(tree, &order, domain, leaf)
}

/// Evaluates `tree` given a precomputed parent-first topological `order`.
fn evaluate_with_order<D, F>(
    tree: &AttackTree,
    order: &[NodeIdx],
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

/// Dependency-tracked incremental evaluation cache over an [`AttackTree`]
/// (rules A1, A2, D10).
///
/// Keeps derived node values and derivations separate from [`AttackTree`]
/// alongside each node's unique position in [`AttackTree::topological_order`]
/// (`rank[parent] < rank[child]` for every directed edge `parent -> child`).
///
/// When one or more step ratings change, [`recompute_steps`](Self::recompute_steps)
/// re-evaluates only the changed steps and the ancestor gates whose input
/// values changed, visiting each affected gate at most once in bottom-up
/// topological rank order and cutting off propagation as soon as a gate's
/// output value equals its previous value. The updated [`Evaluation`] is
/// bit-for-bit identical to running [`evaluate`] from scratch.
///
/// ```
/// use aegis_model::{
///     evaluate, AttackTree, GateKind, MaxMin, ModelError, Node, NodeId, TreeEvalCache,
/// };
///
/// # fn main() -> Result<(), ModelError> {
/// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
/// let left = tree.add_child(tree.root(), NodeId::new_v7(), Node::gate(GateKind::And, "left"))?;
/// let a = tree.add_child(left, NodeId::new_v7(), Node::step("a"))?;
/// let b = tree.add_child(left, NodeId::new_v7(), Node::step("b"))?;
/// let c = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("c"))?;
///
/// let mut ratings = vec![0_u8; tree.len()];
/// ratings[a.index()] = 2;
/// ratings[b.index()] = 4;
/// ratings[c.index()] = 1;
///
/// let domain = MaxMin::<u8>::new();
/// let mut cache = TreeEvalCache::new(&tree, &domain, |i| ratings.get(i.index()).copied())?;
/// assert_eq!(*cache.evaluation().root_value(), 2);
///
/// // Raise step `a` from 2 to 5: `left = min(5, 4) = 4`, `root = max(4, 1) = 4`.
/// ratings[a.index()] = 5;
/// let recomputed = cache.recompute_steps(&tree, &domain, [a], |i| ratings.get(i.index()).copied())?;
/// assert_eq!(recomputed, 3); // step `a`, gate `left`, and `root` (step `b` and `c` untouched)
/// assert_eq!(*cache.evaluation().root_value(), 4);
/// assert_eq!(
///     cache.evaluation(),
///     &evaluate(&tree, &domain, |i| ratings.get(i.index()).copied())?
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEvalCache<V> {
    eval: Evaluation<V>,
    rank: Box<[u32]>,
    edge_count: usize,
}

impl<V> TreeEvalCache<V> {
    /// Builds an incremental evaluation cache for `tree` over `domain`.
    ///
    /// Time O(V + E) plus V leaf evaluations, space O(V).
    ///
    /// # Errors
    ///
    /// Same as [`evaluate`]: [`ModelError::Cycle`],
    /// [`ModelError::MissingLeafValue`], or [`ModelError::EmptyGate`].
    ///
    /// ```
    /// use aegis_model::{AttackTree, MaxMin, ModelError, Node, NodeId, TreeEvalCache};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let cache = TreeEvalCache::new(&tree, &MaxMin::<u8>::new(), |_| Some(3))?;
    /// assert_eq!(*cache.evaluation().root_value(), 3);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new<D, F>(tree: &AttackTree, domain: &D, leaf: F) -> Result<Self, ModelError>
    where
        D: AttributeDomain<Value = V>,
        F: FnMut(NodeIdx) -> Option<V>,
    {
        let order = tree.topological_order()?;
        let mut rank = vec![0_u32; tree.len()];
        for (pos, &idx) in order.iter().enumerate() {
            if let Some(slot) = rank.get_mut(idx.index()) {
                *slot = u32::try_from(pos).unwrap_or(u32::MAX);
            }
        }
        let eval = evaluate_with_order(tree, &order, domain, leaf)?;
        Ok(Self {
            eval,
            rank: rank.into_boxed_slice(),
            edge_count: count_edges(tree),
        })
    }

    /// Borrows the current memoized [`Evaluation`]. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, MaxMin, ModelError, Node, NodeId, TreeEvalCache};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let cache = TreeEvalCache::new(&tree, &MaxMin::<u8>::new(), |_| Some(7))?;
    /// assert_eq!(cache.evaluation().get(tree.root()), Some(&7));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn evaluation(&self) -> &Evaluation<V> {
        &self.eval
    }

    /// Consumes the cache and returns the inner [`Evaluation`] without
    /// cloning. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AttackTree, MaxMin, ModelError, Node, NodeId, TreeEvalCache};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("only"))?;
    /// let cache = TreeEvalCache::new(&tree, &MaxMin::<u8>::new(), |_| Some(7))?;
    /// let eval = cache.into_evaluation();
    /// assert_eq!(*eval.root_value(), 7);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn into_evaluation(self) -> Evaluation<V> {
        self.eval
    }

    /// Incrementally recomputes `steps` and any ancestor gates in `tree`
    /// whose input values changed, returning the total number of nodes
    /// (steps + gates) re-evaluated.
    ///
    /// Each unique step in `steps` is queried via `leaf` once before `self`
    /// is mutated, so an error leaves `self` unchanged. Ancestor gates are
    /// visited in bottom-up topological rank order at most once (rule A2),
    /// and upward propagation stops at any gate whose folded value equals
    /// its previous value (after updating its [`Derivation`]).
    ///
    /// If `tree` was structurally modified since [`TreeEvalCache::new`]
    /// (node count, root, or edge count differs), `recompute_steps` falls
    /// back to a full rebuild after validating `steps`.
    ///
    /// Time O((|S| + A) log(|S| + A) + sum_{u in A} (children(u) + parents(u)))
    /// where `S` is the unique set of input steps and `A` is the set of
    /// ancestor gates whose at least one child value changed; for a single
    /// step rating change, O(depth x fan-out of changed path) (rule A1).
    ///
    /// # Errors
    ///
    /// - [`ModelError::UnknownNode`] if any index in `steps` is not a step
    ///   node in `tree`;
    /// - [`ModelError::MissingLeafValue`] if `leaf` returns `None` for any
    ///   step in `steps`.
    ///
    /// ```
    /// use aegis_model::{
    ///     AttackTree, GateKind, MaxMin, ModelError, Node, NodeId, TreeEvalCache,
    /// };
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "root"))?;
    /// let hi = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("hi"))?;
    /// let lo = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("lo"))?;
    /// let domain = MaxMin::<u8>::new();
    /// let mut cache = TreeEvalCache::new(&tree, &domain, |i| Some(if i == hi { 4 } else { 1 }))?;
    ///
    /// // Changing `lo` from 1 to 2 updates `lo` and folds `root`, which stays 4.
    /// let count = cache.recompute_steps(&tree, &domain, [lo], |i| Some(if i == hi { 4 } else { 2 }))?;
    /// assert_eq!(count, 2);
    /// assert_eq!(*cache.evaluation().root_value(), 4);
    /// # Ok(())
    /// # }
    /// ```
    pub fn recompute_steps<D, I, F>(
        &mut self,
        tree: &AttackTree,
        domain: &D,
        steps: I,
        leaf: F,
    ) -> Result<usize, ModelError>
    where
        D: AttributeDomain<Value = V>,
        V: PartialEq + Clone,
        I: IntoIterator<Item = NodeIdx>,
        F: FnMut(NodeIdx) -> Option<V>,
    {
        let mut touched = Vec::new();
        let (recomputed, _) =
            self.recompute_steps_recording(tree, domain, steps, leaf, &mut touched)?;
        Ok(recomputed)
    }

    /// Same as [`recompute_steps`](Self::recompute_steps), additionally
    /// recording every node whose value or derivation changed in `touched`
    /// and returning `(recomputed_nodes, was_full_rebuild)`.
    pub(crate) fn recompute_steps_recording<D, I, F>(
        &mut self,
        tree: &AttackTree,
        domain: &D,
        steps: I,
        mut leaf: F,
        touched: &mut Vec<NodeIdx>,
    ) -> Result<(usize, bool), ModelError>
    where
        D: AttributeDomain<Value = V>,
        V: PartialEq + Clone,
        I: IntoIterator<Item = NodeIdx>,
        F: FnMut(NodeIdx) -> Option<V>,
    {
        touched.clear();
        let input_steps: Vec<NodeIdx> = steps.into_iter().collect();
        for &step in &input_steps {
            match tree.get(step) {
                Some(Node::Step { .. }) => {}
                Some(Node::Gate { .. }) | None => return Err(ModelError::UnknownNode(step)),
            }
        }

        if !self.topology_matches(tree) {
            *self = Self::new(tree, domain, leaf)?;
            return Ok((tree.len(), true));
        }

        let mut ranked_steps: Vec<(u32, NodeIdx)> = Vec::with_capacity(input_steps.len());
        for step in input_steps {
            let Some(&r) = self.rank.get(step.index()) else {
                *self = Self::new(tree, domain, leaf)?;
                return Ok((tree.len(), true));
            };
            if self.eval.derivation(step) != Some(Derivation::Leaf) {
                *self = Self::new(tree, domain, leaf)?;
                return Ok((tree.len(), true));
            }
            ranked_steps.push((r, step));
        }
        ranked_steps.sort_unstable_by(|a, b| b.cmp(a));
        ranked_steps.dedup();

        // Evaluate and validate all dirty steps before mutating `self.eval`.
        let mut staged: Vec<(NodeIdx, V)> = Vec::with_capacity(ranked_steps.len());
        for &(_, step) in &ranked_steps {
            match leaf(step) {
                Some(v) => staged.push((step, v)),
                None => return Err(ModelError::MissingLeafValue(id_of(tree, step)?)),
            }
        }

        let mut heap: BinaryHeap<(u32, NodeIdx)> = BinaryHeap::new();
        let mut recomputed = staged.len();
        for (step, val) in staged {
            let (val_changed, any_changed) = self.eval.set_node(step, val, Derivation::Leaf);
            if any_changed {
                touched.push(step);
            }
            if val_changed {
                for p in tree.parents(step) {
                    if let Some(&pr) = self.rank.get(p.index()) {
                        heap.push((pr, p));
                    }
                }
            }
        }

        while let Some((r, u)) = heap.pop() {
            while heap.peek() == Some(&(r, u)) {
                heap.pop();
            }
            let Some(Node::Gate { kind, children, .. }) = tree.get(u) else {
                // Topology changed beneath the cache; rebuild from scratch.
                *self = Self::new(tree, domain, leaf)?;
                touched.clear();
                return Ok((tree.len(), true));
            };
            let rule = *kind;
            let folded = match rule {
                GateKind::Or => fold_with(
                    |c| self.eval.get(c),
                    children,
                    rule,
                    |a, v| domain.or_kept(a, v),
                ),
                GateKind::And => fold_with(
                    |c| self.eval.get(c),
                    children,
                    rule,
                    |a, v| domain.and_kept(a, v),
                ),
                GateKind::SeqAnd => fold_with(
                    |c| self.eval.get(c),
                    children,
                    rule,
                    |a, v| domain.seq_and_kept(a, v),
                ),
            };
            let (value, how) = match folded {
                Ok(Some(done)) => done,
                Ok(None) => return Err(ModelError::EmptyGate(id_of(tree, u)?)),
                Err(missing) => return Err(ModelError::UnknownNode(missing)),
            };
            recomputed += 1;
            let (val_changed, any_changed) = self.eval.set_node(u, value, how);
            if any_changed {
                touched.push(u);
            }
            if val_changed {
                for p in tree.parents(u) {
                    if let Some(&pr) = self.rank.get(p.index()) {
                        heap.push((pr, p));
                    }
                }
            }
        }

        Ok((recomputed, false))
    }

    fn topology_matches(&self, tree: &AttackTree) -> bool {
        tree.len() == self.rank.len()
            && tree.root() == self.eval.root
            && count_edges(tree) == self.edge_count
    }
}

fn count_edges(tree: &AttackTree) -> usize {
    tree.edge_count()
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
    combine: impl FnMut(V, &V) -> (V, Kept),
) -> Result<Option<(V, Derivation)>, NodeIdx> {
    fold_with(
        |c: NodeIdx| values.get(c.index()).and_then(Option::as_ref),
        children,
        rule,
        combine,
    )
}

fn fold_with<'a, V: Clone + 'a>(
    value: impl Fn(NodeIdx) -> Option<&'a V>,
    children: &[NodeIdx],
    rule: GateKind,
    mut combine: impl FnMut(V, &V) -> (V, Kept),
) -> Result<Option<(V, Derivation)>, NodeIdx> {
    let get = |c: NodeIdx| value(c).ok_or(c);
    let mut rest = children.iter();
    let Some(&first) = rest.next() else {
        return Ok(None);
    };
    let mut acc = get(first)?.clone();
    let mut how = Derivation::Selected { rule, child: first };
    for &c in rest {
        let (next, kept) = combine(acc, get(c)?);
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

    #[test]
    fn tree_eval_cache_single_step_and_early_cutoff() -> Result<(), ModelError> {
        // Root(OR) -> Mid(OR) -> [s1, s2], plus Root -> s3.
        let mut tree = AttackTree::new(
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "root"),
        )?;
        let mid = tree.add_child(
            tree.root(),
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "mid"),
        )?;
        let s1 = tree.add_child(mid, crate::id::NodeId::new_v7(), Node::step("s1"))?;
        let s2 = tree.add_child(mid, crate::id::NodeId::new_v7(), Node::step("s2"))?;
        let s3 = tree.add_child(tree.root(), crate::id::NodeId::new_v7(), Node::step("s3"))?;

        let d = MaxMin::<u8>::new();
        let mut cache = TreeEvalCache::new(&tree, &d, |i| {
            Some(if i == s1 {
                4
            } else if i == s2 {
                1
            } else if i == s3 {
                3
            } else {
                0
            })
        })?;
        assert_eq!(*cache.evaluation().root_value(), 4);

        // Changing s2 from 1 to 2 updates s2 and folds mid (stays 4 -> early cutoff before root!).
        let recomputed = cache.recompute_steps(&tree, &d, [s2], |i| {
            Some(if i == s1 {
                4
            } else if i == s2 {
                2
            } else if i == s3 {
                3
            } else {
                0
            })
        })?;
        assert_eq!(recomputed, 2); // s2 + mid; root is not folded because mid stayed 4
        let oracle = evaluate(&tree, &d, |i| {
            Some(if i == s1 {
                4
            } else if i == s2 {
                2
            } else if i == s3 {
                3
            } else {
                0
            })
        })?;
        assert_eq!(cache.evaluation(), &oracle);
        Ok(())
    }

    #[test]
    fn tree_eval_cache_tie_break_updates_derivation_on_value_cutoff() -> Result<(), ModelError> {
        // Root(OR) -> Mid(OR) -> [s1, s2].
        // Initially s1 = 2, s2 = 4 -> Mid = 4 (selected s2), Root = 4.
        // Raise s1 to 4 -> Mid stays 4 (early cutoff before Root!), but Mid's derivation
        // shifts from Selected(s2) to Selected(s1) because s1 is the earlier child.
        let mut tree = AttackTree::new(
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "root"),
        )?;
        let mid = tree.add_child(
            tree.root(),
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "mid"),
        )?;
        let s1 = tree.add_child(mid, crate::id::NodeId::new_v7(), Node::step("s1"))?;
        let s2 = tree.add_child(mid, crate::id::NodeId::new_v7(), Node::step("s2"))?;

        let d = MaxMin::<u8>::new();
        let mut cache = TreeEvalCache::new(&tree, &d, |i| Some(if i == s1 { 2 } else { 4 }))?;
        assert_eq!(
            cache.evaluation().derivation(mid),
            Some(Derivation::Selected {
                rule: GateKind::Or,
                child: s2
            })
        );

        let recomputed = cache.recompute_steps(&tree, &d, [s1], |_| Some(4))?;
        assert_eq!(recomputed, 2); // s1 + mid; root not refolded because mid's value stayed 4
        assert_eq!(
            cache.evaluation().derivation(mid),
            Some(Derivation::Selected {
                rule: GateKind::Or,
                child: s1
            })
        );
        let oracle = evaluate(&tree, &d, |_| Some(4))?;
        assert_eq!(cache.evaluation(), &oracle);
        Ok(())
    }

    #[test]
    fn tree_eval_cache_shared_dag_visits_each_ancestor_once() -> Result<(), ModelError> {
        // Diamond DAG: Root(AND) -> [Left(OR), Right(OR)], both linking shared step `s`.
        let mut tree = AttackTree::new(
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::And, "root"),
        )?;
        let left = tree.add_child(
            tree.root(),
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "left"),
        )?;
        let right = tree.add_child(
            tree.root(),
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "right"),
        )?;
        let s = tree.add_child(left, crate::id::NodeId::new_v7(), Node::step("shared"))?;
        tree.link(right, s)?;

        let d = MaxMin::<u8>::new();
        let mut cache = TreeEvalCache::new(&tree, &d, |_| Some(1))?;
        assert_eq!(*cache.evaluation().root_value(), 1);

        // Passing duplicate [s, s] still evaluates s once, left once, right once, root once = 4.
        let recomputed = cache.recompute_steps(&tree, &d, [s, s], |_| Some(3))?;
        assert_eq!(recomputed, 4);
        let oracle = evaluate(&tree, &d, |_| Some(3))?;
        assert_eq!(cache.evaluation(), &oracle);
        Ok(())
    }

    #[test]
    fn tree_eval_cache_error_rollback_leaves_cache_unchanged() -> Result<(), ModelError> {
        let mut tree = AttackTree::new(
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "root"),
        )?;
        let s1 = tree.add_child(tree.root(), crate::id::NodeId::new_v7(), Node::step("s1"))?;
        let s2 = tree.add_child(tree.root(), crate::id::NodeId::new_v7(), Node::step("s2"))?;
        let d = MaxMin::<u8>::new();
        let mut cache = TreeEvalCache::new(&tree, &d, |_| Some(2))?;
        let before = cache.evaluation().clone();

        // Gate passed as a step -> UnknownNode, cache untouched.
        let err = cache.recompute_steps(&tree, &d, [tree.root()], |_| Some(5));
        assert_eq!(err, Err(ModelError::UnknownNode(tree.root())));
        assert_eq!(cache.evaluation(), &before);

        // Missing leaf value on second step -> MissingLeafValue, first step not mutated.
        let s2_id = tree.id_of(s2).ok_or(ModelError::UnknownNode(s2))?;
        let err = cache.recompute_steps(
            &tree,
            &d,
            [s1, s2],
            |i| if i == s1 { Some(9) } else { None },
        );
        assert_eq!(err, Err(ModelError::MissingLeafValue(s2_id)));
        assert_eq!(cache.evaluation(), &before);
        Ok(())
    }

    #[test]
    fn tree_eval_cache_structural_fallback_and_into_evaluation() -> Result<(), ModelError> {
        let mut tree = AttackTree::new(
            crate::id::NodeId::new_v7(),
            Node::gate(GateKind::Or, "root"),
        )?;
        let s1 = tree.add_child(tree.root(), crate::id::NodeId::new_v7(), Node::step("s1"))?;
        let d = MaxMin::<u8>::new();
        let mut cache = TreeEvalCache::new(&tree, &d, |_| Some(2))?;

        // Mutate topology by adding a second step, then call recompute_steps:
        // cache detects the topology change and rebuilds cleanly.
        let s2 = tree.add_child(tree.root(), crate::id::NodeId::new_v7(), Node::step("s2"))?;
        let recomputed =
            cache.recompute_steps(&tree, &d, [s1, s2], |i| Some(if i == s2 { 7 } else { 2 }))?;
        assert_eq!(recomputed, tree.len());
        let oracle = evaluate(&tree, &d, |i| Some(if i == s2 { 7 } else { 2 }))?;
        assert_eq!(&cache.into_evaluation(), &oracle);
        Ok(())
    }

    #[test]
    fn tree_eval_cache_100k_nodes_single_step_under_5ms() -> Result<(), ModelError> {
        // Balanced 4-ary tree with 100,001 nodes (25,000 internal gates + 75,001 leaf steps).
        const GATES: usize = 25_000;
        const ARITY: usize = 4;
        let mut tree = AttackTree::new(crate::id::NodeId::new_v7(), Node::gate(GateKind::Or, "g"))?;
        let mut queue = Vec::with_capacity(GATES * ARITY + 1);
        queue.push(tree.root());
        let mut gates_created = 1usize;
        let mut target_step = None;

        let mut head = 0usize;
        while head < queue.len() {
            let parent = queue[head];
            head += 1;
            for c in 0..ARITY {
                let id = crate::id::NodeId::new_v7();
                if gates_created < GATES {
                    let kind = if (gates_created + c).is_multiple_of(2) {
                        GateKind::Or
                    } else {
                        GateKind::And
                    };
                    let g = tree.add_child(parent, id, Node::gate(kind, "g"))?;
                    queue.push(g);
                    gates_created += 1;
                } else {
                    let s = tree.add_child(parent, id, Node::step("s"))?;
                    target_step = Some(s);
                }
            }
        }
        assert_eq!(tree.len(), 100_001);
        let step = target_step.ok_or(ModelError::UnknownNode(tree.root()))?;

        let d = MaxMin::<u8>::new();
        let mut cache = TreeEvalCache::new(&tree, &d, |_| Some(1))?;

        let start = std::time::Instant::now();
        let recomputed =
            cache.recompute_steps(&tree, &d, [step], |i| Some(if i == step { 3 } else { 1 }))?;
        let elapsed = start.elapsed();

        // Depth of a 4-ary tree with 25,000 gates is at most 9, so <= 10 nodes are recomputed.
        assert!(recomputed <= 12, "recomputed={recomputed}");
        assert!(
            elapsed < std::time::Duration::from_millis(5),
            "100k-node incremental recomputation took {elapsed:?} (> 5ms budget)"
        );
        Ok(())
    }
}
