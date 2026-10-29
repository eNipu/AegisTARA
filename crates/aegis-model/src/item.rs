//! Item definition entities: system components, interfaces, data flows, and
//! trust boundaries (job 074, ARC-02, Decision D15).
//!
//! Built and mutated only through [`Project::apply`](crate::Project::apply).

use std::sync::Arc;

use crate::id::{ComponentIdx, InterfaceIdx};
use crate::methodology::InterfaceKind;
use crate::provenance::Provenance;

/// Maximum depth of the component containment hierarchy (`parent` chain).
///
/// ```
/// assert_eq!(aegis_model::MAX_COMPONENT_DEPTH, 16);
/// ```
pub const MAX_COMPONENT_DEPTH: usize = 16;

/// Maximum number of [`Component`]s that one [`TrustBoundary`] may contain.
///
/// ```
/// assert_eq!(aegis_model::MAX_TRUST_BOUNDARY_MEMBERS, 64);
/// ```
pub const MAX_TRUST_BOUNDARY_MEMBERS: usize = 64;

/// A system component (ECU, subsystem, or external actor) in the item
/// definition of a [`Project`](crate::Project) (ARC-02).
///
/// A component may optionally sit inside a parent component (for example a
/// hardware security module inside a gateway ECU), forming a cycle-free
/// containment hierarchy of depth at most [`MAX_COMPONENT_DEPTH`].
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let id = ComponentId::new_v7();
/// project.apply(Op::AddComponent {
///     id,
///     name: "Central Gateway".into(),
///     parent: None,
///     provenance: Provenance::default(),
/// })?;
/// let idx = project.component_idx(&id).ok_or("missing")?;
/// let comp = project.component(idx).ok_or("missing")?;
/// assert_eq!(comp.name(), "Central Gateway");
/// assert_eq!(comp.parent(), None);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Component {
    pub(crate) name: Arc<str>,
    pub(crate) parent: Option<ComponentIdx>,
    pub(crate) provenance: Provenance,
}

impl Component {
    /// Returns the component's name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id,
    ///     name: "Telematics Unit".into(),
    ///     parent: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, c) = project.components().next().ok_or("missing")?;
    /// assert_eq!(c.name(), "Telematics Unit");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parent component's arena index, or `None` for a top-level
    /// component. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let parent = ComponentId::new_v7();
    /// let child = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: parent, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddComponent {
    ///     id: child, name: "HSM".into(), parent: Some(parent), provenance: Provenance::default(),
    /// })?;
    /// let p_idx = project.component_idx(&parent);
    /// let c_idx = project.component_idx(&child).ok_or("missing")?;
    /// assert_eq!(project.component(c_idx).and_then(|c| c.parent()), p_idx);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn parent(&self) -> Option<ComponentIdx> {
        self.parent
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this component. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ComponentId::new_v7();
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// project.apply(Op::AddComponent {
    ///     id, name: "ECU".into(), parent: None, provenance: prov.clone(),
    /// })?;
    /// let (_, _, c) = project.components().next().ok_or("missing")?;
    /// assert_eq!(c.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// An external or internal communication interface exposed by a [`Component`],
/// typed by a pack-defined [`InterfaceKind`] (ARC-02).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let comp = ComponentId::new_v7();
/// project.apply(Op::AddComponent {
///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
/// })?;
/// let kind = m.interface_kinds().next().ok_or("no kind")?;
/// let id = InterfaceId::new_v7();
/// project.apply(Op::AddInterface {
///     id,
///     name: "OBD-II Port".into(),
///     component: comp,
///     kind,
///     provenance: Provenance::default(),
/// })?;
/// let idx = project.interface_idx(&id).ok_or("missing")?;
/// let iface = project.interface(idx).ok_or("missing")?;
/// assert_eq!(iface.name(), "OBD-II Port");
/// assert_eq!(iface.kind(), kind);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interface {
    pub(crate) name: Arc<str>,
    pub(crate) component: ComponentIdx,
    pub(crate) kind: InterfaceKind,
    pub(crate) provenance: Provenance,
}

impl Interface {
    /// Returns the interface's name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN0".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let (_, _, iface) = project.interfaces().next().ok_or("missing")?;
    /// assert_eq!(iface.name(), "CAN0");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the arena index of the [`Component`] that exposes this
    /// interface. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN0".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let comp_idx = project.component_idx(&comp).ok_or("missing")?;
    /// let (_, _, iface) = project.interfaces().next().ok_or("missing")?;
    /// assert_eq!(iface.component(), comp_idx);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn component(&self) -> ComponentIdx {
        self.component
    }

    /// Returns the pack-defined [`InterfaceKind`] of this interface. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN0".into(), component: comp, kind, provenance: Provenance::default(),
    /// })?;
    /// let (_, _, iface) = project.interfaces().next().ok_or("missing")?;
    /// assert_eq!(iface.kind(), kind);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn kind(&self) -> InterfaceKind {
        self.kind
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this interface. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, InterfaceId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let comp = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let kind = m.interface_kinds().next().ok_or("no kind")?;
    /// let id = InterfaceId::new_v7();
    /// let prov = Provenance::human("alice", 100);
    /// project.apply(Op::AddInterface {
    ///     id, name: "CAN0".into(), component: comp, kind, provenance: prov.clone(),
    /// })?;
    /// let (_, _, iface) = project.interfaces().next().ok_or("missing")?;
    /// assert_eq!(iface.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// A directed data flow from one [`Component`] to another, optionally over an
/// [`Interface`] (ARC-02).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let c1 = ComponentId::new_v7();
/// let c2 = ComponentId::new_v7();
/// project.apply(Op::AddComponent {
///     id: c1, name: "TCU".into(), parent: None, provenance: Provenance::default(),
/// })?;
/// project.apply(Op::AddComponent {
///     id: c2, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
/// })?;
/// let id = DataFlowId::new_v7();
/// project.apply(Op::AddDataFlow {
///     id,
///     name: "Diagnostic request".into(),
///     from: c1,
///     to: c2,
///     interface: None,
///     provenance: Provenance::default(),
/// })?;
/// let (_, _, df) = project.data_flows().next().ok_or("missing")?;
/// assert_eq!(df.name(), "Diagnostic request");
/// assert_eq!(df.interface(), None);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFlow {
    pub(crate) name: Arc<str>,
    pub(crate) from: ComponentIdx,
    pub(crate) to: ComponentIdx,
    pub(crate) interface: Option<InterfaceIdx>,
    pub(crate) provenance: Provenance,
}

impl DataFlow {
    /// Returns the data flow's name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c1 = ComponentId::new_v7();
    /// let c2 = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c1, name: "TCU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddComponent {
    ///     id: c2, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "Wheel speed".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, df) = project.data_flows().next().ok_or("missing")?;
    /// assert_eq!(df.name(), "Wheel speed");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the arena index of the source [`Component`]. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c1 = ComponentId::new_v7();
    /// let c2 = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c1, name: "TCU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddComponent {
    ///     id: c2, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "Wheel speed".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let c1_idx = project.component_idx(&c1).ok_or("missing")?;
    /// let (_, _, df) = project.data_flows().next().ok_or("missing")?;
    /// assert_eq!(df.from(), c1_idx);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn from(&self) -> ComponentIdx {
        self.from
    }

    /// Returns the arena index of the destination [`Component`]. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c1 = ComponentId::new_v7();
    /// let c2 = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c1, name: "TCU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddComponent {
    ///     id: c2, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "Wheel speed".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let c2_idx = project.component_idx(&c2).ok_or("missing")?;
    /// let (_, _, df) = project.data_flows().next().ok_or("missing")?;
    /// assert_eq!(df.to(), c2_idx);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn to(&self) -> ComponentIdx {
        self.to
    }

    /// Returns the arena index of the [`Interface`] over which the flow
    /// travels, if any. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c1 = ComponentId::new_v7();
    /// let c2 = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c1, name: "TCU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddComponent {
    ///     id: c2, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = DataFlowId::new_v7();
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "Wheel speed".into(), from: c1, to: c2, interface: None,
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, df) = project.data_flows().next().ok_or("missing")?;
    /// assert_eq!(df.interface(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn interface(&self) -> Option<InterfaceIdx> {
        self.interface
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this data flow. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, DataFlowId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c1 = ComponentId::new_v7();
    /// let c2 = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c1, name: "TCU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddComponent {
    ///     id: c2, name: "Gateway".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = DataFlowId::new_v7();
    /// let prov = Provenance::human("alice", 100);
    /// project.apply(Op::AddDataFlow {
    ///     id, name: "Wheel speed".into(), from: c1, to: c2, interface: None,
    ///     provenance: prov.clone(),
    /// })?;
    /// let (_, _, df) = project.data_flows().next().ok_or("missing")?;
    /// assert_eq!(df.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// A trust boundary grouping [`Component`]s that share a privilege or security
/// domain (ARC-02).
///
/// Member components are stored in canonical [`ComponentId`](crate::ComponentId)
/// order, at most [`MAX_TRUST_BOUNDARY_MEMBERS`].
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance, TrustBoundaryId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let c = ComponentId::new_v7();
/// project.apply(Op::AddComponent {
///     id: c, name: "Brake ECU".into(), parent: None, provenance: Provenance::default(),
/// })?;
/// let id = TrustBoundaryId::new_v7();
/// project.apply(Op::AddTrustBoundary {
///     id,
///     name: "Chassis CAN domain".into(),
///     components: vec![c],
///     provenance: Provenance::default(),
/// })?;
/// let (_, _, tb) = project.trust_boundaries().next().ok_or("missing")?;
/// assert_eq!(tb.name(), "Chassis CAN domain");
/// assert_eq!(tb.components().len(), 1);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustBoundary {
    pub(crate) name: Arc<str>,
    pub(crate) components: Vec<ComponentIdx>,
    pub(crate) provenance: Provenance,
}

impl TrustBoundary {
    /// Returns the trust boundary's name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Op, Project, Provenance, TrustBoundaryId};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Infotainment zone".into(), components: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, tb) = project.trust_boundaries().next().ok_or("missing")?;
    /// assert_eq!(tb.name(), "Infotainment zone");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the arena indices of the member [`Component`]s in canonical
    /// [`ComponentId`](crate::ComponentId) order. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance, TrustBoundaryId};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![c], provenance: Provenance::default(),
    /// })?;
    /// let c_idx = project.component_idx(&c).ok_or("missing")?;
    /// let (_, _, tb) = project.trust_boundaries().next().ok_or("missing")?;
    /// assert_eq!(tb.components(), &[c_idx]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn components(&self) -> &[ComponentIdx] {
        &self.components
    }

    /// Returns `true` if `comp` is a member of this trust boundary. Time O(k),
    /// k <= [`MAX_TRUST_BOUNDARY_MEMBERS`].
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, Methodology, Op, Project, Provenance, TrustBoundaryId};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let c = ComponentId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: c, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// let id = TrustBoundaryId::new_v7();
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![c], provenance: Provenance::default(),
    /// })?;
    /// let c_idx = project.component_idx(&c).ok_or("missing")?;
    /// let (_, _, tb) = project.trust_boundaries().next().ok_or("missing")?;
    /// assert!(tb.contains(c_idx));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn contains(&self, comp: ComponentIdx) -> bool {
        self.components.contains(&comp)
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this trust boundary. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Op, Project, Provenance, TrustBoundaryId};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = TrustBoundaryId::new_v7();
    /// let prov = Provenance::human("alice", 100);
    /// project.apply(Op::AddTrustBoundary {
    ///     id, name: "Zone A".into(), components: vec![], provenance: prov.clone(),
    /// })?;
    /// let (_, _, tb) = project.trust_boundaries().next().ok_or("missing")?;
    /// assert_eq!(tb.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}
