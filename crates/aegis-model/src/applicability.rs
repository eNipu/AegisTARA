//! Applicability conditions: the project-level list of states and events that
//! element links may name as needed.
//!
//! A condition is a named, described boolean fact. Conditions change only
//! through [`Op`](crate::Op)s on a [`Project`](crate::Project). Elements and
//! steps may need conditions; [`Project::applicability`] derives which of
//! them are excluded while a needed condition is off, and why (job 134).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::description::ElementRef;
use crate::eval::{Applicable, AttributeDomain, Lifted, evaluate};
use crate::id::{
    AssumptionId, ConditionId, ConditionIdx, NodeId, NodeIdx, ThreatId, TreeId, TreeIdx,
};
use crate::item::{Component, ItemRef, MAX_COMPONENT_DEPTH};
use crate::project::Project;
use crate::provenance::Provenance;
use crate::tree::AttackTree;

/// Maximum number of live conditions in one project.
///
/// ```
/// assert_eq!(aegis_model::MAX_CONDITIONS, 256);
/// ```
pub const MAX_CONDITIONS: usize = 256;

/// Maximum number of conditions one element or step may need (job 133).
pub const MAX_CONDITIONS_PER_ELEMENT: usize = 16;

/// Maximum number of live assumptions in one project.
pub const MAX_ASSUMPTIONS: usize = 256;

/// Maximum number of trees and steps one assumption may exclude.
pub const MAX_ASSUMPTION_EXCLUDES: usize = 256;

/// Maximum number of assumptions one cybersecurity claim may reference.
pub const MAX_CLAIM_ASSUMPTIONS: usize = 64;

/// Stable public reference to one attack-tree step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepRef {
    /// The attack tree containing the step.
    pub tree: TreeId,
    /// The step node inside `tree`.
    pub node: NodeId,
}

/// An applicability assumption in a [`Project`](crate::Project), changed only
/// through [`Project::apply`](crate::Project::apply).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assumption {
    pub(crate) name: Arc<str>,
    pub(crate) holds: bool,
    pub(crate) excludes_trees: Vec<TreeIdx>,
    pub(crate) excludes_steps: Vec<StepRef>,
    pub(crate) provenance: Provenance,
}

impl Assumption {
    /// The assumption's name. Time O(1).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the assumption currently holds. Time O(1).
    #[must_use]
    pub fn holds(&self) -> bool {
        self.holds
    }

    /// Attack-tree indices excluded while this assumption holds. Time O(1).
    #[must_use]
    pub fn excludes_trees(&self) -> &[TreeIdx] {
        &self.excludes_trees
    }

    /// Attack-tree steps excluded while this assumption holds. Time O(1).
    #[must_use]
    pub fn excludes_steps(&self) -> &[StepRef] {
        &self.excludes_steps
    }

    /// Provenance of the last change to this assumption. Time O(1).
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// A condition in a [`Project`](crate::Project), changed only through
/// [`Project::apply`](crate::Project::apply).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ConditionId, Methodology, Op, Project, Provenance};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut p = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let id = ConditionId::new_v7();
/// let prov = Provenance::human("alice", 1_700_000_000_000);
/// p.apply(Op::AddCondition {
///     id,
///     name: "Airbag deploys".into(),
///     description: "The airbag deploys in a frontal crash".into(),
///     present: true,
///     provenance: prov.clone(),
/// })?;
/// let c = p.condition(p.condition_idx(&id).ok_or("missing")?).ok_or("missing")?;
/// assert_eq!(c.name(), "Airbag deploys");
/// assert_eq!(c.description(), "The airbag deploys in a frontal crash");
/// assert!(c.present());
/// assert_eq!(c.provenance(), &prov);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    pub(crate) name: Arc<str>,
    pub(crate) description: Arc<str>,
    pub(crate) present: bool,
    pub(crate) provenance: Provenance,
}

impl Condition {
    /// The condition's name. Time O(1). See the [`Condition`] example.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The condition's plain-text description. Time O(1).
    /// See the [`Condition`] example.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether the condition is present in the project. Time O(1).
    /// See the [`Condition`] example.
    #[must_use]
    pub fn present(&self) -> bool {
        self.present
    }

    /// Provenance of the last change to this condition. Time O(1).
    /// See the [`Condition`] example.
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// Why an element or attack tree node is excluded. Variants are ordered, so
/// reason lists sort the same way every time.
///
/// ```
/// use aegis_model::{ConditionId, Reason};
///
/// assert!(Reason::ConditionOff(ConditionId::new_v7()) < Reason::AllPathsExcluded);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// Any element or step: it needs this condition and the condition is off.
    ConditionOff(ConditionId),
    /// A tree or step: an assumption that excludes it currently holds.
    AssumptionHolds(AssumptionId),
    /// A child component, interface, asset, threat or tree: its owner is
    /// excluded (the parent component, component, asset or threat).
    OwnerExcluded(ElementRef),
    /// A step: the component or interface it goes through is excluded.
    ViaExcluded(ItemRef),
    /// A gate, or a tree through its root: no path through it applies.
    AllPathsExcluded,
}

/// Which elements and attack tree nodes are excluded, and why. Derived from
/// a [`Project`] by [`Project::applicability`] and never stored (D10). Only
/// excluded elements and nodes have entries; reason lists are sorted and
/// unique.
///
/// ```
/// use aegis_model::Applicability;
///
/// let none = Applicability::default();
/// assert!(none.is_empty());
/// assert_eq!(none.excluded_elements().count(), 0);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Applicability {
    elements: BTreeMap<ElementRef, Vec<Reason>>,
    trees: BTreeMap<TreeId, TreeExclusions>,
}

/// The excluded nodes of one tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TreeExclusions {
    nodes: BTreeMap<NodeId, Vec<Reason>>,
    /// One slot per `NodeIdx`, for O(1) lookups.
    excluded: Vec<bool>,
}

impl Applicability {
    /// Whether nothing is excluded. Time O(1).
    ///
    /// ```
    /// use aegis_model::Applicability;
    ///
    /// assert!(Applicability::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.trees.is_empty()
    }

    /// Whether element `e` is excluded. Time O(log n).
    ///
    /// ```
    /// use aegis_model::{Applicability, ElementRef};
    ///
    /// assert!(!Applicability::default().is_excluded(ElementRef::Project));
    /// ```
    #[must_use]
    pub fn is_excluded(&self, e: ElementRef) -> bool {
        self.elements.contains_key(&e)
    }

    /// Why element `e` is excluded: sorted, unique, empty when it is not.
    /// Time O(log n).
    ///
    /// ```
    /// use aegis_model::{Applicability, ElementRef};
    ///
    /// assert!(Applicability::default().reasons(ElementRef::Project).is_empty());
    /// ```
    #[must_use]
    pub fn reasons(&self, e: ElementRef) -> &[Reason] {
        self.elements.get(&e).map_or(&[][..], Vec::as_slice)
    }

    /// Whether node `node` of `tree` is excluded. Time O(log n).
    ///
    /// ```
    /// use aegis_model::{Applicability, NodeId, TreeId};
    ///
    /// assert!(!Applicability::default().is_node_excluded(TreeId::new_v7(), NodeId::new_v7()));
    /// ```
    #[must_use]
    pub fn is_node_excluded(&self, tree: TreeId, node: NodeId) -> bool {
        self.trees
            .get(&tree)
            .is_some_and(|t| t.nodes.contains_key(&node))
    }

    /// Why node `node` of `tree` is excluded: sorted, unique, empty when it
    /// is not. Time O(log n).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AttackTree, ConditionId, GateKind, Methodology, Node, NodeId, Op, Project, Provenance, Reason, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut p = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// let (t, root, step, cf) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7(), ConditionId::new_v7());
    /// let mut tree = AttackTree::new(root, Node::gate(GateKind::And, "root"))?;
    /// tree.add_child(tree.root(), step, Node::step("open"))?;
    /// p.apply(Op::AddAttackTree { id: t, tree, step_feasibility: vec![], provenance: prov.clone() })?;
    /// p.apply(Op::AddCondition {
    ///     id: cf, name: "Option fitted".into(), description: "".into(), present: false, provenance: prov.clone(),
    /// })?;
    /// p.apply(Op::NeedStepCondition { tree: t, node: step, condition: cf, provenance: prov })?;
    /// let a = p.applicability();
    /// assert_eq!(a.node_reasons(t, step), &[Reason::ConditionOff(cf)]);
    /// assert_eq!(a.node_reasons(t, root), &[Reason::AllPathsExcluded]);
    /// assert!(a.is_node_excluded(t, step));
    /// assert_eq!(a.excluded_nodes().count(), 2);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn node_reasons(&self, tree: TreeId, node: NodeId) -> &[Reason] {
        self.trees
            .get(&tree)
            .and_then(|t| t.nodes.get(&node))
            .map_or(&[][..], Vec::as_slice)
    }

    /// Every excluded element with its reasons, in `ElementRef` order.
    /// Time O(1) per item.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, ConditionId, ElementRef, InterfaceId, Methodology, Op, Project, Provenance, Reason};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut p = Project::new(Arc::clone(&m));
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// let (c, i, cf) = (ComponentId::new_v7(), InterfaceId::new_v7(), ConditionId::new_v7());
    /// p.apply(Op::AddComponent { id: c, name: "ECU".into(), parent: None, provenance: prov.clone() })?;
    /// let kind = m.interface_kinds().next().ok_or("kind")?;
    /// p.apply(Op::AddInterface { id: i, name: "CAN".into(), component: c, kind, provenance: prov.clone() })?;
    /// p.apply(Op::AddCondition {
    ///     id: cf, name: "Option fitted".into(), description: "".into(), present: false, provenance: prov.clone(),
    /// })?;
    /// p.apply(Op::NeedCondition { element: ElementRef::Component(c), condition: cf, provenance: prov })?;
    /// let a = p.applicability();
    /// assert!(a.is_excluded(ElementRef::Interface(i)));
    /// let all: Vec<_> = a.excluded_elements().collect();
    /// assert_eq!(all, vec![
    ///     (ElementRef::Component(c), &[Reason::ConditionOff(cf)][..]),
    ///     (ElementRef::Interface(i), &[Reason::OwnerExcluded(ElementRef::Component(c))][..]),
    /// ]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn excluded_elements(&self) -> impl Iterator<Item = (ElementRef, &[Reason])> {
        self.elements.iter().map(|(e, r)| (*e, r.as_slice()))
    }

    /// Every excluded node with its reasons, in tree then node ID order.
    /// Time O(1) per item.
    ///
    /// ```
    /// use aegis_model::{Applicability};
    ///
    /// assert_eq!(Applicability::default().excluded_nodes().count(), 0);
    /// ```
    pub fn excluded_nodes(&self) -> impl Iterator<Item = (TreeId, NodeId, &[Reason])> {
        self.trees
            .iter()
            .flat_map(|(t, x)| x.nodes.iter().map(move |(n, r)| (*t, *n, r.as_slice())))
    }

    /// Whether `self` and `other` agree on threat `threat` and, when given,
    /// on tree `tree` and every node of it. Time O(log n + nodes of `tree`).
    pub(crate) fn same_for_threat(
        &self,
        other: &Self,
        threat: ThreatId,
        tree: Option<TreeId>,
    ) -> bool {
        let same = |e: ElementRef| self.elements.get(&e) == other.elements.get(&e);
        same(ElementRef::Threat(threat))
            && tree.is_none_or(|t| {
                same(ElementRef::Tree(t))
                    && self.trees.get(&t).map(|x| &x.nodes) == other.trees.get(&t).map(|x| &x.nodes)
            })
    }

    /// Trees with a tree-level or node-level exclusion, possibly repeated.
    /// Time O(1) per item.
    pub(crate) fn touched_trees(&self) -> impl Iterator<Item = TreeId> + '_ {
        let whole = self.elements.keys().filter_map(|e| match e {
            ElementRef::Tree(t) => Some(*t),
            _ => None,
        });
        self.trees.keys().copied().chain(whole)
    }

    /// The per-`NodeIdx` exclusion flags of `tree`, `None` when no node of
    /// it is excluded. Time O(log n).
    pub(crate) fn node_flags(&self, tree: TreeId) -> Option<&[bool]> {
        self.trees.get(&tree).map(|t| t.excluded.as_slice())
    }

    /// Whether the node at `idx` of `tree` is excluded. Time O(log n).
    pub(crate) fn node_excluded_at(&self, tree: TreeId, idx: NodeIdx) -> bool {
        self.trees
            .get(&tree)
            .and_then(|t| t.excluded.get(idx.index()))
            .copied()
            .unwrap_or(false)
    }
}

/// The unit domain: every node has the value `()`. Lifted by [`Applicable`],
/// it computes only which gates are excluded.
struct Presence;

impl AttributeDomain for Presence {
    type Value = ();
    fn or(&self, (): (), (): &()) {}
    fn and(&self, (): (), (): &()) {}
}

/// Adds `reason` to `e`'s list.
fn push(map: &mut BTreeMap<ElementRef, Vec<Reason>>, e: ElementRef, reason: Reason) {
    map.entry(e).or_default().push(reason);
}

/// Adds `OwnerExcluded(owner)` to `child` when `owner` is excluded.
fn inherit(
    ex: &mut BTreeMap<ElementRef, Vec<Reason>>,
    child: ElementRef,
    owner: Option<ElementRef>,
) {
    if let Some(owner) = owner.filter(|o| ex.contains_key(o)) {
        push(ex, child, Reason::OwnerExcluded(owner));
    }
}

impl Project {
    /// Which elements and attack tree nodes are excluded because a needed
    /// condition is off, and why. Derived, never stored (D10); the result
    /// depends only on public IDs.
    ///
    /// Rules: an element or step is excluded by its own off conditions; an
    /// excluded component excludes its child components, interfaces and
    /// bound assets; an excluded asset excludes its threats; a tree is
    /// excluded when every threat using it is; a step through an excluded
    /// component or interface is excluded; a gate is excluded when no path
    /// through it applies (OR: every child, AND/SAND: any child), and the
    /// tree too when its root is. Goals and claims are excluded only by
    /// their own conditions.
    ///
    /// Time O(c) when every condition is present, else O(e log e + n) for
    /// e elements and n tree nodes.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, ConditionId, ElementRef, Methodology, Op, Project, Provenance, Reason};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut p = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// let (parent, child, cf) = (ComponentId::new_v7(), ComponentId::new_v7(), ConditionId::new_v7());
    /// p.apply(Op::AddComponent { id: parent, name: "Body".into(), parent: None, provenance: prov.clone() })?;
    /// p.apply(Op::AddComponent { id: child, name: "Door".into(), parent: Some(parent), provenance: prov.clone() })?;
    /// p.apply(Op::AddCondition {
    ///     id: cf, name: "Option fitted".into(), description: "".into(), present: true, provenance: prov.clone(),
    /// })?;
    /// p.apply(Op::NeedCondition { element: ElementRef::Component(parent), condition: cf, provenance: prov.clone() })?;
    /// assert!(p.applicability().is_empty());
    ///
    /// p.apply(Op::SetConditionPresent { id: cf, present: false, provenance: prov })?;
    /// let a = p.applicability();
    /// assert_eq!(a.reasons(ElementRef::Component(child)), &[Reason::OwnerExcluded(ElementRef::Component(parent))]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn applicability(&self) -> Applicability {
        if self.conditions().all(|(_, _, c)| c.present())
            && self.assumptions().all(|(_, _, a)| !a.holds())
        {
            return Applicability::default();
        }
        let off = |list: &[_]| -> Vec<Reason> {
            list.iter()
                .filter(|&&i| self.condition(i).is_some_and(|c| !c.present()))
                .filter_map(|&i| self.condition_id(i).map(Reason::ConditionOff))
                .collect()
        };
        let mut elements: BTreeMap<ElementRef, Vec<Reason>> = BTreeMap::new();
        for (e, list) in self.condition_link_entries() {
            for r in off(list) {
                push(&mut elements, e, r);
            }
        }
        let mut step_assumptions: BTreeMap<TreeId, BTreeMap<NodeId, Vec<AssumptionId>>> =
            BTreeMap::new();
        for (_, aid, a) in self.assumptions() {
            if !a.holds() || elements.contains_key(&ElementRef::Assumption(aid)) {
                continue;
            }
            for &t_idx in a.excludes_trees() {
                if let Some(tree_id) = self.attack_tree_id(t_idx) {
                    push(
                        &mut elements,
                        ElementRef::Tree(tree_id),
                        Reason::AssumptionHolds(aid),
                    );
                }
            }
            for sr in a.excludes_steps() {
                step_assumptions
                    .entry(sr.tree)
                    .or_default()
                    .entry(sr.node)
                    .or_default()
                    .push(aid);
            }
        }
        for list in elements.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        self.propagate_owners(&mut elements);
        let mut trees = BTreeMap::new();
        for (t, id, tree) in self.attack_trees() {
            if let Some(x) = self.tree_exclusions(t, id, tree, &elements, &off, &step_assumptions) {
                if x.excluded
                    .get(tree.root().index())
                    .copied()
                    .unwrap_or(false)
                {
                    push(
                        &mut elements,
                        ElementRef::Tree(id),
                        Reason::AllPathsExcluded,
                    );
                }
                trees.insert(id, x);
            }
        }
        for list in elements.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        Applicability { elements, trees }
    }

    /// Rule c: owner edges, parents before children.
    fn propagate_owners(&self, ex: &mut BTreeMap<ElementRef, Vec<Reason>>) {
        let mut comps: Vec<_> = self
            .components()
            .map(|(i, id, c)| {
                let mut depth = 0;
                let mut at = c.parent();
                while let Some(p) = at.filter(|_| depth <= MAX_COMPONENT_DEPTH) {
                    depth += 1;
                    at = self.component(p).and_then(Component::parent);
                }
                (depth, i, id)
            })
            .collect();
        comps.sort_unstable();
        for (_, i, id) in comps {
            let parent = self.component(i).and_then(Component::parent);
            let owner = parent.and_then(|p| self.component_id(p));
            inherit(
                ex,
                ElementRef::Component(id),
                owner.map(ElementRef::Component),
            );
        }
        for (_, id, iface) in self.interfaces() {
            let owner = self.component_id(iface.component());
            inherit(
                ex,
                ElementRef::Interface(id),
                owner.map(ElementRef::Component),
            );
        }
        for (_, id, asset) in self.assets() {
            let owner = asset.component().and_then(|c| self.component_id(c));
            inherit(ex, ElementRef::Asset(id), owner.map(ElementRef::Component));
        }
        let mut users: BTreeMap<TreeIdx, Vec<ElementRef>> = BTreeMap::new();
        for (_, id, threat) in self.threat_scenarios() {
            let owner = self.asset_id(threat.asset()).map(ElementRef::Asset);
            inherit(ex, ElementRef::Threat(id), owner);
            if let Some(t) = threat.attack_tree() {
                users.entry(t).or_default().push(ElementRef::Threat(id));
            }
        }
        for (t, threats) in users {
            let Some(tree) = self.attack_tree_id(t).map(ElementRef::Tree) else {
                continue;
            };
            if threats.iter().all(|th| ex.contains_key(th)) {
                for th in threats {
                    push(ex, tree, Reason::OwnerExcluded(th));
                }
            }
        }
    }

    /// Rules d and e for one tree: steps, then gates. `None` when no node
    /// is excluded.
    fn tree_exclusions(
        &self,
        t: TreeIdx,
        tree_id: TreeId,
        tree: &AttackTree,
        ex: &BTreeMap<ElementRef, Vec<Reason>>,
        off: &impl Fn(&[ConditionIdx]) -> Vec<Reason>,
        step_assumptions: &BTreeMap<TreeId, BTreeMap<NodeId, Vec<AssumptionId>>>,
    ) -> Option<TreeExclusions> {
        let mut out = TreeExclusions {
            nodes: BTreeMap::new(),
            excluded: vec![false; tree.len()],
        };
        let tree_steps = step_assumptions.get(&tree_id);
        for (n, nid, node) in tree.iter() {
            if node.gate_kind().is_some() {
                continue;
            }
            let mut reasons = off(self.step_conditions(t, n));
            if let Some(via) = self.step_via(t, n) {
                let owner = match via {
                    ItemRef::Component(c) => ElementRef::Component(c),
                    ItemRef::Interface(i) => ElementRef::Interface(i),
                };
                if ex.contains_key(&owner) {
                    reasons.push(Reason::ViaExcluded(via));
                }
            }
            if let Some(aids) = tree_steps.and_then(|nodes| nodes.get(&nid)) {
                reasons.extend(aids.iter().copied().map(Reason::AssumptionHolds));
            }
            if !reasons.is_empty() {
                reasons.sort_unstable();
                reasons.dedup();
                out.nodes.insert(nid, reasons);
                if let Some(slot) = out.excluded.get_mut(n.index()) {
                    *slot = true;
                }
            }
        }
        if out.nodes.is_empty() {
            return None;
        }
        let leaf = |i: NodeIdx| {
            let excluded = out.excluded.get(i.index()).copied().unwrap_or(false);
            Some(if excluded {
                Lifted::Excluded
            } else {
                Lifted::Value(())
            })
        };
        if let Ok(eval) = evaluate(tree, &Applicable(Presence), leaf) {
            for (n, nid, node) in tree.iter() {
                let gate_off = eval.get(n).is_some_and(Lifted::is_excluded);
                if node.gate_kind().is_some() && gate_off {
                    out.nodes.insert(nid, vec![Reason::AllPathsExcluded]);
                    if let Some(slot) = out.excluded.get_mut(n.index()) {
                        *slot = true;
                    }
                }
            }
        }
        Some(out)
    }
}
