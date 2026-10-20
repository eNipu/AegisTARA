//! Bottom-up evaluation of an attack tree over a pluggable attribute domain.
//!
//! An attribute domain (Mauw and Oostdijk, "Foundations of Attack Trees",
//! ICISC 2005) says how each gate kind combines the values of its children,
//! for example "OR = most feasible child, AND = least feasible child". The
//! caller supplies the value of every leaf step. [`evaluate`] computes a
//! value for every node in one pass over a topological order, so each node
//! is evaluated exactly once, also when subtrees are shared (rule A2).
//!
//! This module uses only the public API of [`AttackTree`], so it can move to
//! a later engine crate unchanged. See `docs/adr/0005-tree-evaluation.md`.

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
}

/// The value of every node of one tree, from [`evaluate`].
///
/// Derived data (rule D10): it is kept apart from the [`AttackTree`] and can
/// be dropped and recomputed at any time. Values are stored in arena order,
/// so lookups by [`NodeIdx`] are O(1). Only meaningful for the tree it was
/// computed from.
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
/// Time O(V + E) plus V calls of `leaf` (steps only) and E - G calls of the
/// domain (G = gates). Extra space O(V) for the order and the values. No
/// recursion, so deep trees cannot overflow the stack.
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

    for &idx in order.iter().rev() {
        let value = match tree.get(idx) {
            Some(Node::Step { .. }) => match leaf(idx) {
                Some(v) => v,
                None => return Err(ModelError::MissingLeafValue(id_of(tree, idx)?)),
            },
            Some(Node::Gate { kind, children, .. }) => {
                let folded = match kind {
                    GateKind::Or => fold(&values, children, |acc, v| domain.or(acc, v)),
                    GateKind::And => fold(&values, children, |acc, v| domain.and(acc, v)),
                    GateKind::SeqAnd => fold(&values, children, |acc, v| domain.seq_and(acc, v)),
                };
                match folded {
                    Ok(Some(v)) => v,
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
    }

    let root = tree.root();
    match values.get_mut(root.index()).and_then(Option::take) {
        Some(root_value) => Ok(Evaluation {
            values,
            root,
            root_value,
        }),
        // INVARIANT: the order contains every node, including the root, so
        // its slot was filled above.
        None => Err(ModelError::UnknownNode(root)),
    }
}

/// Left fold of the children's memoized values: a clone of the first, then
/// `combine(acc, &v)` for the rest, in stored order.
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
    mut combine: impl FnMut(V, &V) -> V,
) -> Result<Option<V>, NodeIdx> {
    let value = |c: NodeIdx| values.get(c.index()).and_then(Option::as_ref).ok_or(c);
    let mut rest = children.iter();
    let Some(&first) = rest.next() else {
        return Ok(None);
    };
    let mut acc = value(first)?.clone();
    for &c in rest {
        acc = combine(acc, value(c)?);
    }
    Ok(Some(acc))
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
        if *next > acc { next.clone() } else { acc }
    }

    /// Minimum; keeps `acc` on a tie. Time O(1) plus at most one clone.
    fn and(&self, acc: T, next: &T) -> T {
        if *next < acc { next.clone() } else { acc }
    }
}

/// Minimum attack time in abstract units (`u64`): OR = minimum (the fastest
/// alternative), AND = maximum (children run in parallel), SAND = sum
/// (children run one after another). The sum saturates at `u64::MAX`
/// instead of overflowing. Each combine is O(1).
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
        acc.min(*next)
    }

    /// Maximum (parallel). Time O(1).
    fn and(&self, acc: u64, next: &u64) -> u64 {
        acc.max(*next)
    }

    /// Saturating sum (sequential). Time O(1).
    fn seq_and(&self, acc: u64, next: &u64) -> u64 {
        acc.saturating_add(*next)
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
        let got = fold(&values, &children, |a, b| a.max(*b));
        assert_eq!(got, Err(children[1]));
        assert_eq!(fold(&values, &[], |a: u8, b| a.max(*b)), Ok(None));
    }
}
