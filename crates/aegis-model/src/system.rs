//! Derived systems: which systems every element belongs to or is used by
//! (119c).
//!
//! Only a top-level component stores its system
//! ([`Component::system`](crate::Component::system)). [`Project::system_use`]
//! derives, in one pass over the links the project already has, a
//! [`SystemSet`] for every other element. Nothing is stored and no risk
//! number depends on it.

use crate::element::ElementKind;
use crate::id::{
    AssetIdx, ClaimIdx, ComponentIdx, ControlIdx, DamageIdx, DataFlowIdx, ElementId, GoalIdx,
    InterfaceIdx, ThreatIdx, TreeIdx, TrustBoundaryIdx,
};
use crate::item::{ItemRef, MAX_COMPONENT_DEPTH};
use crate::project::Project;

/// A set of systems, at most [`MAX_SYSTEMS`](crate::MAX_SYSTEMS). Bit `i`
/// stands for the system with rank `i` in ID order (see
/// [`SystemUse::systems`]). Every operation is O(1).
///
/// ```
/// use aegis_model::SystemSet;
///
/// assert!(SystemSet::EMPTY.is_empty());
/// assert_eq!(SystemSet::EMPTY.len(), 0);
/// // An element with nothing to derive from shows in every view.
/// assert!(SystemSet::EMPTY.shows_in(SystemSet::EMPTY));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SystemSet(u64);

impl SystemSet {
    /// The empty set.
    pub const EMPTY: SystemSet = SystemSet(0);

    /// The set of the system with this rank, or empty when the rank is 64
    /// or more. Time O(1).
    const fn rank(rank: usize) -> SystemSet {
        if rank < 64 {
            SystemSet(1 << rank)
        } else {
            SystemSet::EMPTY
        }
    }

    /// Whether the set holds no system. Time O(1).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The number of systems in the set. Time O(1).
    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The systems in either set. Time O(1).
    #[must_use]
    pub const fn union(self, other: SystemSet) -> SystemSet {
        SystemSet(self.0 | other.0)
    }

    /// Whether the two sets share a system. Time O(1).
    #[must_use]
    pub const fn intersects(self, other: SystemSet) -> bool {
        self.0 & other.0 != 0
    }

    /// Whether an element with this set shows when the client shows the
    /// systems in `view`. An element with nothing to derive from shows in
    /// every view. Time O(1).
    #[must_use]
    pub const fn shows_in(self, view: SystemSet) -> bool {
        self.is_empty() || self.intersects(view)
    }
}

/// The systems of every element of a project, from
/// [`Project::system_use`]. Space one word per element.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ComponentId, ElementId, ElementKind, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let system = ElementId::new_v7();
/// let ecu = ComponentId::new_v7();
/// project.apply(Op::AddElement {
///     id: system, kind: ElementKind::System, name: "Brake".into(),
///     provenance: Provenance::default(),
/// })?;
/// project.apply(Op::AddComponent {
///     id: ecu, name: "ECU".into(), parent: None, provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetComponentSystem {
///     id: ecu, system: Some(system), provenance: Provenance::default(),
/// })?;
/// let usage = project.system_use();
/// let idx = project.component_idx(&ecu).ok_or("missing")?;
/// let set = usage.component(idx);
/// assert_eq!(set, usage.set_of([system]));
/// assert_eq!(usage.ids(set).collect::<Vec<_>>(), [system]);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemUse {
    systems: Vec<ElementId>,
    components: Vec<SystemSet>,
    interfaces: Vec<SystemSet>,
    data_flows: Vec<SystemSet>,
    trust_boundaries: Vec<SystemSet>,
    assets: Vec<SystemSet>,
    threats: Vec<SystemSet>,
    trees: Vec<SystemSet>,
    damages: Vec<SystemSet>,
    controls: Vec<SystemSet>,
    goals: Vec<SystemSet>,
    claims: Vec<SystemSet>,
}

/// The set at index `i`, or empty. Time O(1).
fn get(sets: &[SystemSet], i: usize) -> SystemSet {
    sets.get(i).copied().unwrap_or_default()
}

/// Adds `set` to the set at index `i`, growing `sets` as needed. Time O(1)
/// amortized.
fn add(sets: &mut Vec<SystemSet>, i: usize, set: SystemSet) {
    if set.is_empty() {
        return;
    }
    if sets.len() <= i {
        sets.resize(i + 1, SystemSet::EMPTY);
    }
    if let Some(s) = sets.get_mut(i) {
        *s = s.union(set);
    }
}

impl SystemUse {
    /// The live systems in rank (ID) order. Time O(1).
    #[must_use]
    pub fn systems(&self) -> &[ElementId] {
        &self.systems
    }

    /// The set of the given systems; an ID that is not a live system is
    /// ignored. Time O(k log s) for k IDs and s systems.
    #[must_use]
    pub fn set_of(&self, ids: impl IntoIterator<Item = ElementId>) -> SystemSet {
        ids.into_iter()
            .filter_map(|id| self.systems.binary_search(&id).ok())
            .fold(SystemSet::EMPTY, |acc, rank| {
                acc.union(SystemSet::rank(rank))
            })
    }

    /// The systems in `set`, in rank order. Time O(s) for s systems.
    pub fn ids(&self, set: SystemSet) -> impl Iterator<Item = ElementId> + '_ {
        self.systems
            .iter()
            .enumerate()
            .filter(move |(rank, _)| SystemSet::rank(*rank).intersects(set))
            .map(|(_, id)| *id)
    }

    /// The systems of a component. Time O(1).
    #[must_use]
    pub fn component(&self, idx: ComponentIdx) -> SystemSet {
        get(&self.components, idx.index())
    }

    /// The systems of an interface. Time O(1).
    #[must_use]
    pub fn interface(&self, idx: InterfaceIdx) -> SystemSet {
        get(&self.interfaces, idx.index())
    }

    /// The systems a data flow touches. Time O(1).
    #[must_use]
    pub fn data_flow(&self, idx: DataFlowIdx) -> SystemSet {
        get(&self.data_flows, idx.index())
    }

    /// The systems a trust boundary touches. Time O(1).
    #[must_use]
    pub fn trust_boundary(&self, idx: TrustBoundaryIdx) -> SystemSet {
        get(&self.trust_boundaries, idx.index())
    }

    /// The systems of an asset. Time O(1).
    #[must_use]
    pub fn asset(&self, idx: AssetIdx) -> SystemSet {
        get(&self.assets, idx.index())
    }

    /// The systems of a threat scenario. Time O(1).
    #[must_use]
    pub fn threat(&self, idx: ThreatIdx) -> SystemSet {
        get(&self.threats, idx.index())
    }

    /// The systems that use an attack tree. Time O(1).
    #[must_use]
    pub fn tree(&self, idx: TreeIdx) -> SystemSet {
        get(&self.trees, idx.index())
    }

    /// The systems that use a damage scenario. Time O(1).
    #[must_use]
    pub fn damage(&self, idx: DamageIdx) -> SystemSet {
        get(&self.damages, idx.index())
    }

    /// The systems that use a security control. Time O(1).
    #[must_use]
    pub fn control(&self, idx: ControlIdx) -> SystemSet {
        get(&self.controls, idx.index())
    }

    /// The systems that use a cybersecurity goal. Time O(1).
    #[must_use]
    pub fn goal(&self, idx: GoalIdx) -> SystemSet {
        get(&self.goals, idx.index())
    }

    /// The systems that use a cybersecurity claim. Time O(1).
    #[must_use]
    pub fn claim(&self, idx: ClaimIdx) -> SystemSet {
        get(&self.claims, idx.index())
    }
}

impl Project {
    /// The system of the component's top-level ancestor, or `None` (also
    /// for an unknown index). Time O(depth).
    #[must_use]
    pub fn component_system(&self, idx: ComponentIdx) -> Option<ElementId> {
        let mut at = self.component(idx)?;
        for _ in 0..=MAX_COMPONENT_DEPTH {
            match at.parent() {
                None => return at.system(),
                Some(parent) => at = self.component(parent)?,
            }
        }
        None
    }

    /// Derives the systems of every element from the links the project
    /// already has. A component has the system of its top-level ancestor;
    /// an interface and an asset have their component's; a threat has its
    /// asset's; a data flow and a trust boundary have the union over their
    /// components; an attack tree, a damage scenario, a goal and a claim
    /// have the union over the threats that link them; a control has the
    /// union over its targets, the trees in which it mitigates a step and
    /// the goals that list it. An element with nothing to derive from has
    /// the empty set.
    ///
    /// Time O(components x depth + interfaces + flows + boundary members +
    /// assets + threats + damage links + tree nodes + mitigations + goal
    /// links), space one word per element.
    #[must_use]
    pub fn system_use(&self) -> SystemUse {
        let systems: Vec<ElementId> = self
            .elements(ElementKind::System)
            .map(|(id, _)| id)
            .collect();
        if systems.is_empty() {
            return SystemUse::default();
        }
        let mut u = SystemUse {
            systems,
            ..SystemUse::default()
        };
        for (idx, _, _) in self.components() {
            let set = u.set_of(self.component_system(idx));
            add(&mut u.components, idx.index(), set);
        }
        for (idx, _, i) in self.interfaces() {
            let set = u.component(i.component());
            add(&mut u.interfaces, idx.index(), set);
        }
        for (idx, _, f) in self.data_flows() {
            let set = u.component(f.from()).union(u.component(f.to()));
            add(&mut u.data_flows, idx.index(), set);
        }
        for (idx, _, b) in self.trust_boundaries() {
            let set = b
                .components()
                .iter()
                .fold(SystemSet::EMPTY, |acc, c| acc.union(u.component(*c)));
            add(&mut u.trust_boundaries, idx.index(), set);
        }
        for (idx, _, a) in self.assets() {
            let set = a.component.map_or(SystemSet::EMPTY, |c| u.component(c));
            add(&mut u.assets, idx.index(), set);
        }
        for (idx, _, t) in self.threat_scenarios() {
            let set = u.asset(t.asset);
            add(&mut u.threats, idx.index(), set);
            for d in &t.damages {
                add(&mut u.damages, d.index(), set);
            }
            if let Some(tree) = t.attack_tree {
                add(&mut u.trees, tree.index(), set);
            }
            if let Some(goal) = t.goal {
                add(&mut u.goals, goal.index(), set);
            }
            if let Some(claim) = t.claim {
                add(&mut u.claims, claim.index(), set);
            }
        }
        for (idx, _, _) in self.security_controls() {
            let set = self
                .control_targets(idx)
                .fold(SystemSet::EMPTY, |acc, target| {
                    let set = match target {
                        ItemRef::Component(id) => self.component_idx(&id).map(|c| u.component(c)),
                        ItemRef::Interface(id) => self.interface_idx(&id).map(|i| u.interface(i)),
                    };
                    acc.union(set.unwrap_or_default())
                });
            add(&mut u.controls, idx.index(), set);
        }
        for (tree_idx, _, tree) in self.attack_trees() {
            let set = u.tree(tree_idx);
            if set.is_empty() {
                continue;
            }
            for (node, _, _) in tree.iter() {
                for m in self.step_mitigations(tree_idx, node) {
                    add(&mut u.controls, m.control().index(), set);
                }
            }
        }
        for (idx, _, g) in self.cybersecurity_goals() {
            let set = u.goal(idx);
            for c in g.controls() {
                add(&mut u.controls, c.index(), set);
            }
        }
        u
    }
}
