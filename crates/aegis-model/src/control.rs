//! Security controls, step mitigations, risk treatment decisions,
//! cybersecurity goals, and cybersecurity claims.

use std::fmt;
use std::sync::Arc;

use crate::id::{ControlId, ControlIdx, NodeId, NodeIdx, PartyIdx, TreeIdx};
use crate::item::{ItemIdx, ItemRef};
use crate::methodology::{AttackType, ControlKind, FeasibilityLevel};
use crate::provenance::Provenance;

/// Lifecycle status of a [`SecurityControl`] (CFG-10, D28).
///
/// Controls default to [`Planned`](Self::Planned) so pre-existing projects keep
/// identical initial and residual risk numbers:
///
/// - [`Implemented`](Self::Implemented): already built into the item; counts in
///   both **Initial** and **Residual** evaluations (mitigations, rules, and
///   goal coverage).
/// - [`Planned`](Self::Planned): proposed by the TARA; counts only in
///   **Residual** evaluation (mitigations, rules, and goal coverage).
/// - [`Off`](Self::Off): disabled / excluded; counts in neither evaluation nor
///   goal coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ControlStatus {
    /// Already present in the item; counts in both Initial and Residual risk.
    Implemented,
    /// Planned by this TARA; counts in Residual risk only (default).
    #[default]
    Planned,
    /// Disabled; counts in neither Initial nor Residual risk.
    Off,
}

impl ControlStatus {
    /// All statuses in display order.
    pub const ALL: [Self; 3] = [Self::Implemented, Self::Planned, Self::Off];

    /// Stable wire/storage key (`"implemented"`, `"planned"`, `"off"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Planned => "planned",
            Self::Off => "off",
        }
    }

    /// Parses a wire key into a [`ControlStatus`].
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "implemented" => Some(Self::Implemented),
            "planned" => Some(Self::Planned),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// Whether this control counts in the Initial (inherent) evaluation.
    #[must_use]
    pub const fn counts_for_initial(self) -> bool {
        matches!(self, Self::Implemented)
    }

    /// Whether this control counts in the Residual evaluation and goal coverage.
    #[must_use]
    pub const fn counts_for_residual(self) -> bool {
        matches!(self, Self::Implemented | Self::Planned)
    }

    /// Whether this control counts for the given evaluation mode (`false` = Initial,
    /// `true` = Residual).
    #[must_use]
    pub const fn counts_for(self, residual: bool) -> bool {
        if residual {
            self.counts_for_residual()
        } else {
            self.counts_for_initial()
        }
    }

    /// Human-readable English label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Implemented => "Implemented",
            Self::Planned => "Planned",
            Self::Off => "Off",
        }
    }
}

impl fmt::Display for ControlStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Maximum number of [`StepMitigation`]s attached to one attack step.
pub const MAX_STEP_MITIGATIONS: usize = 16;

/// Maximum number of [`SecurityControl`]s a [`CybersecurityGoal`] may link to.
///
/// ```
/// assert_eq!(aegis_model::MAX_GOAL_CONTROLS, 16);
/// ```
pub const MAX_GOAL_CONTROLS: usize = 16;

/// Maximum number of components and interfaces a [`SecurityControl`] may be
/// applied to via [`Op::ApplyControl`](crate::Op::ApplyControl) (CFG-03, D26).
///
/// ```
/// assert_eq!(aegis_model::MAX_CONTROL_TARGETS, 64);
/// ```
pub const MAX_CONTROL_TARGETS: usize = 64;

/// Maximum number of [`ControlRule`]s a [`SecurityControl`] may carry via
/// [`Op::SetControlRule`](crate::Op::SetControlRule) (CFG-03, D26).
///
/// ```
/// assert_eq!(aegis_model::MAX_CONTROL_RULES, 16);
/// ```
pub const MAX_CONTROL_RULES: usize = 16;

/// One rule on a [`SecurityControl`] mapping an [`AttackType`] to a residual
/// [`FeasibilityLevel`] ceiling (CFG-03, D26).
///
/// 2 bytes, `Copy`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ControlId, ControlRule, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
/// let vlow = m.feasibility_level("very-low").ok_or("no very-low")?;
/// let mut project = Project::new(m);
/// let id = ControlId::new_v7();
/// project.apply(Op::AddSecurityControl {
///     id,
///     name: "JTAG disabled".into(),
///     provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetControlRule {
///     control: id,
///     attack_type: phys,
///     ceiling: Some(vlow),
///     provenance: Provenance::default(),
/// })?;
/// let idx = project.security_control_idx(&id).ok_or("missing")?;
/// let c = project.security_control(idx).ok_or("missing")?;
/// assert_eq!(c.rules(), &[ControlRule::new(phys, vlow)]);
/// assert_eq!(c.rules()[0].attack_type(), phys);
/// assert_eq!(c.rules()[0].ceiling(), vlow);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ControlRule {
    pub(crate) attack_type: AttackType,
    pub(crate) ceiling: FeasibilityLevel,
}

impl ControlRule {
    /// Creates a rule mapping `attack_type` to `ceiling`. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlRule, Methodology};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
    /// let low = m.feasibility_level("low").ok_or("no low")?;
    /// let r = ControlRule::new(phys, low);
    /// assert_eq!(r.attack_type(), phys);
    /// assert_eq!(r.ceiling(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn new(attack_type: AttackType, ceiling: FeasibilityLevel) -> Self {
        Self {
            attack_type,
            ceiling,
        }
    }

    /// Returns the [`AttackType`] matched by this rule. Time O(1).
    ///
    /// ```
    /// use aegis_model::{ControlRule, Methodology};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
    /// let low = m.feasibility_level("low").ok_or("no low")?;
    /// assert_eq!(ControlRule::new(phys, low).attack_type(), phys);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn attack_type(&self) -> AttackType {
        self.attack_type
    }

    /// Returns the [`FeasibilityLevel`] ceiling imposed by this rule. Time O(1).
    ///
    /// ```
    /// use aegis_model::{ControlRule, Methodology};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
    /// let low = m.feasibility_level("low").ok_or("no low")?;
    /// assert_eq!(ControlRule::new(phys, low).ceiling(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn ceiling(&self) -> FeasibilityLevel {
        self.ceiling
    }
}

/// How a [`StepCeiling`] reaches an attack step (CFG-05, D26).
///
/// ```
/// use aegis_model::{CeilingSource, ComponentId, ItemRef, Methodology};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
/// let comp = ItemRef::Component(ComponentId::new_v7());
/// assert!(CeilingSource::StepMitigation < CeilingSource::ControlRule {
///     target: comp,
///     attack_type: phys,
/// });
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CeilingSource {
    /// Attached directly to the step via
    /// [`Op::SetStepMitigation`](crate::Op::SetStepMitigation).
    StepMitigation,
    /// Derived from a [`ControlRule`] on a [`SecurityControl`] applied to
    /// `target` (the step's `via` element or its owning component).
    ControlRule {
        /// The component or interface where the control is applied.
        target: ItemRef,
        /// The step's [`AttackType`] matched by the rule.
        attack_type: AttackType,
    },
}

/// One feasibility ceiling acting on an attack step, returned by
/// [`Project::step_ceilings`](crate::Project::step_ceilings) with the deciding
/// (lowest) ceiling first (CFG-04, CFG-05, D26).
///
/// Holds only typed IDs, arena indices, and methodology scale levels, never
/// user-supplied names (rule R6).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AttackTree, CeilingSource, ComponentId, ControlId, ItemRef, Methodology, Node, NodeId, Op,
///     Project, Provenance, TreeId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
/// let (vlow, high) = (
///     m.feasibility_level("very-low").ok_or("no very-low")?,
///     m.feasibility_level("high").ok_or("no high")?,
/// );
/// let mut project = Project::new(m);
/// let comp = ComponentId::new_v7();
/// let ctrl = ControlId::new_v7();
/// let (tree_id, step) = (TreeId::new_v7(), NodeId::new_v7());
/// project.apply(Op::AddComponent {
///     id: comp, name: "ADAS ECU".into(), parent: None, provenance: Provenance::default(),
/// })?;
/// project.apply(Op::AddSecurityControl {
///     id: ctrl, name: "JTAG disabled".into(), provenance: Provenance::default(),
/// })?;
/// project.apply(Op::ApplyControl {
///     control: ctrl, target: ItemRef::Component(comp), provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetControlRule {
///     control: ctrl, attack_type: phys, ceiling: Some(vlow), provenance: Provenance::default(),
/// })?;
/// let tree = AttackTree::new(step, Node::step("Dump flash"))?;
/// let root_idx = tree.root();
/// project.apply(Op::AddAttackTree {
///     id: tree_id, tree, step_feasibility: vec![(step, high)], provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetStepVia {
///     tree: tree_id, node: step, via: Some(ItemRef::Component(comp)),
///     provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetStepAttackType {
///     tree: tree_id, node: step, attack_type: Some(phys), provenance: Provenance::default(),
/// })?;
/// let t_idx = project.attack_tree_idx(&tree_id).ok_or("no tree")?;
/// let ceilings = project.step_ceilings(t_idx, root_idx);
/// assert_eq!(ceilings.len(), 1);
/// assert_eq!(ceilings[0].control_id(), ctrl);
/// assert_eq!(ceilings[0].feasibility(), vlow);
/// assert_eq!(
///     ceilings[0].source(),
///     CeilingSource::ControlRule { target: ItemRef::Component(comp), attack_type: phys },
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepCeiling {
    pub(crate) control: ControlIdx,
    pub(crate) control_id: ControlId,
    pub(crate) feasibility: FeasibilityLevel,
    pub(crate) source: CeilingSource,
    pub(crate) status: ControlStatus,
}

impl StepCeiling {
    /// Returns the arena index of the [`SecurityControl`] imposing this ceiling.
    /// Time O(1).
    #[must_use]
    pub const fn control(&self) -> ControlIdx {
        self.control
    }

    /// Returns the stable [`ControlId`] of the [`SecurityControl`] imposing this
    /// ceiling. Time O(1).
    #[must_use]
    pub const fn control_id(&self) -> ControlId {
        self.control_id
    }

    /// Returns the [`FeasibilityLevel`] ceiling imposed on the step. Time O(1).
    #[must_use]
    pub const fn feasibility(&self) -> FeasibilityLevel {
        self.feasibility
    }

    /// Returns how this ceiling reaches the step. Time O(1).
    #[must_use]
    pub const fn source(&self) -> CeilingSource {
        self.source
    }

    /// Returns the [`ControlStatus`] of the control imposing this ceiling (CFG-10).
    /// Time O(1).
    #[must_use]
    pub const fn status(&self) -> ControlStatus {
        self.status
    }
}

/// The attack step that decides a tree's residual feasibility, with the
/// security settings that cap it, returned by
/// [`Project::deciding_step`](crate::Project::deciding_step) (CFG-07).
///
/// Holds only typed IDs, arena indices, and methodology scale levels, never
/// user-supplied names (rule R6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecidingStep {
    pub(crate) tree: TreeIdx,
    pub(crate) node: NodeIdx,
    pub(crate) node_id: NodeId,
    pub(crate) rated: FeasibilityLevel,
    pub(crate) residual: FeasibilityLevel,
    pub(crate) ceilings: Vec<StepCeiling>,
}

impl DecidingStep {
    /// Returns the arena index of the tree holding the step. Time O(1).
    #[must_use]
    pub const fn tree(&self) -> TreeIdx {
        self.tree
    }

    /// Returns the node index of the step inside its tree. Time O(1).
    #[must_use]
    pub const fn node(&self) -> NodeIdx {
        self.node
    }

    /// Returns the stable [`NodeId`] of the step. Time O(1).
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the step's own rating, before any ceiling. Time O(1).
    #[must_use]
    pub const fn rated(&self) -> FeasibilityLevel {
        self.rated
    }

    /// Returns the step's feasibility after its ceilings. Time O(1).
    #[must_use]
    pub const fn residual(&self) -> FeasibilityLevel {
        self.residual
    }

    /// Returns the ceilings below [`rated`](Self::rated), the deciding
    /// (lowest) one first. Time O(1).
    #[must_use]
    pub fn ceilings(&self) -> &[StepCeiling] {
        &self.ceilings
    }

    /// Returns `true` if at least one ceiling lowers the step. Time O(1).
    #[must_use]
    pub fn is_capped(&self) -> bool {
        !self.ceilings.is_empty()
    }
}

/// A security control (countermeasure or safeguard) in a
/// [`Project`](crate::Project).
///
/// A control is named once at the project level and can be attached directly
/// to attack steps via [`Op::SetStepMitigation`](crate::Op::SetStepMitigation)
/// or applied as a setting to components and interfaces via
/// [`Op::ApplyControl`](crate::Op::ApplyControl) with rules set via
/// [`Op::SetControlRule`](crate::Op::SetControlRule) to cap matching steps'
/// residual feasibility. [`Op::SetControlKind`](crate::Op::SetControlKind)
/// records which pack preset the control was made from.
///
/// Built and changed only through [`Project::apply`](crate::Project::apply).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(m);
/// let id = ControlId::new_v7();
/// project.apply(Op::AddSecurityControl {
///     id,
///     name: "Authenticated boot".into(),
///     provenance: Provenance::default(),
/// })?;
/// let idx = project.security_control_idx(&id).ok_or("missing")?;
/// let c = project.security_control(idx).ok_or("missing")?;
/// assert_eq!(c.name(), "Authenticated boot");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecurityControl {
    pub(crate) name: Arc<str>,
    pub(crate) targets: Vec<ItemIdx>,
    pub(crate) rules: Vec<ControlRule>,
    pub(crate) kind: Option<ControlKind>,
    pub(crate) status: ControlStatus,
    pub(crate) provenance: Provenance,
}

impl SecurityControl {
    /// Returns the security control's name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "Rate limiter".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// let c = project.security_control(idx).ok_or("missing")?;
    /// assert_eq!(c.name(), "Rate limiter");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the pack preset this control was created from, if any
    /// (CFG-06). Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let boot = m.control_kind("secure-boot").ok_or("no preset")?;
    /// let mut project = Project::new(m);
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "Secure boot".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetControlKind {
    ///     control: id,
    ///     kind: Some(boot),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// let c = project.security_control(idx).ok_or("missing")?;
    /// assert_eq!(c.kind(), Some(boot));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn kind(&self) -> Option<ControlKind> {
        self.kind
    }

    /// Returns the lifecycle status of this control (CFG-10, D28). Defaults to
    /// [`ControlStatus::Planned`]. Time O(1).
    #[must_use]
    pub const fn status(&self) -> ControlStatus {
        self.status
    }

    /// Returns the control's [`ControlRule`]s, sorted by [`AttackType`]. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
    /// let low = m.feasibility_level("low").ok_or("no low")?;
    /// let mut project = Project::new(m);
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "JTAG disabled".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetControlRule {
    ///     control: id,
    ///     attack_type: phys,
    ///     ceiling: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// let c = project.security_control(idx).ok_or("missing")?;
    /// assert_eq!(c.rules().len(), 1);
    /// assert_eq!(c.rule_for(phys), Some(low));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn rules(&self) -> &[ControlRule] {
        &self.rules
    }

    /// Looks up the [`FeasibilityLevel`] ceiling for `attack_type`, if this
    /// control defines one. Time $O(\log k)$ where $k \le 16$.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let phys = m.attack_type("physical-tampering").ok_or("no physical-tampering")?;
    /// let net = m.attack_type("remote-network").ok_or("no remote-network")?;
    /// let low = m.feasibility_level("low").ok_or("no low")?;
    /// let mut project = Project::new(m);
    /// let id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "JTAG disabled".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetControlRule {
    ///     control: id,
    ///     attack_type: phys,
    ///     ceiling: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// let c = project.security_control(idx).ok_or("missing")?;
    /// assert_eq!(c.rule_for(phys), Some(low));
    /// assert_eq!(c.rule_for(net), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn rule_for(&self, attack_type: AttackType) -> Option<FeasibilityLevel> {
        self.rules
            .binary_search_by_key(&attack_type, |r| r.attack_type)
            .ok()
            .map(|i| self.rules[i].ceiling)
    }

    /// Returns the number of components and interfaces this control is applied
    /// to. Use [`Project::control_targets`](crate::Project::control_targets) to
    /// iterate over their [`ItemRef`]s. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ComponentId, ControlId, ItemRef, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let comp = ComponentId::new_v7();
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddComponent {
    ///     id: comp, name: "ECU".into(), parent: None, provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl, name: "Secure boot".into(), provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::ApplyControl {
    ///     control: ctrl, target: ItemRef::Component(comp), provenance: Provenance::default(),
    /// })?;
    /// let idx = project.security_control_idx(&ctrl).ok_or("missing")?;
    /// assert_eq!(project.security_control(idx).ok_or("missing")?.target_count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this security control. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlId, Methodology, Op, Project, Provenance, ProvenanceSource};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ControlId::new_v7();
    /// let prov = Provenance::new("alice", 1_700_000_000_000, ProvenanceSource::Human);
    /// project.apply(Op::AddSecurityControl {
    ///     id,
    ///     name: "Rate limiter".into(),
    ///     provenance: prov.clone(),
    /// })?;
    /// let idx = project.security_control_idx(&id).ok_or("missing")?;
    /// let c = project.security_control(idx).ok_or("missing")?;
    /// assert_eq!(c.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// One [`SecurityControl`] mitigation attached to an attack step, capping
/// that step's residual feasibility at [`feasibility`](Self::feasibility).
///
/// When multiple mitigations are attached to the same step, the step's
/// residual feasibility is the minimum of its inherent feasibility and every
/// attached mitigation's [`feasibility`](Self::feasibility) (hardest for the
/// attacker wins).
///
/// 8 bytes, `Copy`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let high = m.feasibility_level("high").ok_or("no level")?;
/// let low = m.feasibility_level("low").ok_or("no level")?;
///
/// let ctrl_id = ControlId::new_v7();
/// project.apply(Op::AddSecurityControl {
///     id: ctrl_id,
///     name: "Hardware key store".into(),
///     provenance: Provenance::default(),
/// })?;
/// let ctrl_idx = project.security_control_idx(&ctrl_id).ok_or("missing ctrl")?;
///
/// let step_id = NodeId::new_v7();
/// let tree_id = TreeId::new_v7();
/// let tree = AttackTree::new(step_id, Node::step("Extract key"))?;
/// let root_idx = tree.root();
/// project.apply(Op::AddAttackTree {
///     id: tree_id,
///     tree,
///     step_feasibility: vec![(step_id, high)],
///     provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetStepMitigation {
///     tree: tree_id,
///     node: step_id,
///     control: ctrl_id,
///     feasibility: Some(low),
///     provenance: Provenance::default(),
/// })?;
///
/// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing tree")?;
/// let mitigations = project.step_mitigations(tree_idx, root_idx);
/// assert_eq!(mitigations.len(), 1);
/// assert_eq!(mitigations[0].control(), ctrl_idx);
/// assert_eq!(mitigations[0].feasibility(), low);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepMitigation {
    pub(crate) control: ControlIdx,
    pub(crate) feasibility: FeasibilityLevel,
}

impl StepMitigation {
    /// Returns the index of the attached [`SecurityControl`]. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{
    ///     AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// };
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let ctrl_id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl_id,
    ///     name: "Filter".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let step_id = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let tree = AttackTree::new(step_id, Node::step("Inject"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id,
    ///     tree,
    ///     step_feasibility: vec![(step_id, low)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id,
    ///     node: step_id,
    ///     control: ctrl_id,
    ///     feasibility: Some(low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// let ctrl_idx = project.security_control_idx(&ctrl_id).ok_or("missing")?;
    /// assert_eq!(project.step_mitigations(tree_idx, root)[0].control(), ctrl_idx);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn control(self) -> ControlIdx {
        self.control
    }

    /// Returns the residual [`FeasibilityLevel`] cap imposed by this
    /// control on the step. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{
    ///     AttackTree, ControlId, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
    /// };
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let very_low = m.feasibility_level("very-low").ok_or("no level")?;
    /// let ctrl_id = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl_id,
    ///     name: "Filter".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let step_id = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let tree = AttackTree::new(step_id, Node::step("Inject"))?;
    /// let root = tree.root();
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id,
    ///     tree,
    ///     step_feasibility: vec![(step_id, very_low)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetStepMitigation {
    ///     tree: tree_id,
    ///     node: step_id,
    ///     control: ctrl_id,
    ///     feasibility: Some(very_low),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tree_idx = project.attack_tree_idx(&tree_id).ok_or("missing")?;
    /// assert_eq!(project.step_mitigations(tree_idx, root)[0].feasibility(), very_low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility(self) -> FeasibilityLevel {
        self.feasibility
    }
}

/// Risk treatment decision for a [`ThreatScenario`](crate::ThreatScenario).
///
/// Ordered from active elimination/reduction to transfer and retention:
/// `Avoid < Reduce < Share < Retain`.
///
/// ```
/// use aegis_model::RiskTreatment;
///
/// assert_eq!(RiskTreatment::Reduce.as_str(), "reduce");
/// assert_eq!(RiskTreatment::Retain.to_string(), "retain");
/// assert!(RiskTreatment::Avoid < RiskTreatment::Retain);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskTreatment {
    /// Avoid the risk by removing the risk source or disabling the feature.
    Avoid,
    /// Reduce the risk by applying [`SecurityControl`]s.
    Reduce,
    /// Share or transfer the risk to another party.
    Share,
    /// Retain (accept) the risk under documented rationale.
    Retain,
}

impl RiskTreatment {
    /// Returns the canonical wire and display key of this treatment decision.
    /// Time O(1).
    ///
    /// ```
    /// use aegis_model::RiskTreatment;
    ///
    /// assert_eq!(RiskTreatment::Avoid.as_str(), "avoid");
    /// assert_eq!(RiskTreatment::Reduce.as_str(), "reduce");
    /// assert_eq!(RiskTreatment::Share.as_str(), "share");
    /// assert_eq!(RiskTreatment::Retain.as_str(), "retain");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Avoid => "avoid",
            Self::Reduce => "reduce",
            Self::Share => "share",
            Self::Retain => "retain",
        }
    }
}

impl fmt::Display for RiskTreatment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A cybersecurity goal in a [`Project`](crate::Project) (ISO/SAE 21434
/// Clause 15.5).
///
/// A cybersecurity goal is a top-level security requirement associated with one
/// or more threat scenarios treated with [`RiskTreatment::Reduce`] and
/// realized by linking one or more [`SecurityControl`]s (sorted by
/// [`ControlId`](crate::ControlId), unique, at most [`MAX_GOAL_CONTROLS`]).
///
/// Built and changed only through [`Project::apply`](crate::Project::apply).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ControlId, GoalId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let ctrl = ControlId::new_v7();
/// project.apply(Op::AddSecurityControl {
///     id: ctrl,
///     name: "SecOC message authentication".into(),
///     provenance: Provenance::default(),
/// })?;
/// let goal = GoalId::new_v7();
/// project.apply(Op::AddCybersecurityGoal {
///     id: goal,
///     name: "Prevent unauthorized brake actuation commands".into(),
///     controls: vec![ctrl],
///     provenance: Provenance::default(),
/// })?;
/// let idx = project.cybersecurity_goal_idx(&goal).ok_or("missing")?;
/// let g = project.cybersecurity_goal(idx).ok_or("missing")?;
/// assert_eq!(g.name(), "Prevent unauthorized brake actuation commands");
/// assert_eq!(g.controls(), [project.security_control_idx(&ctrl).ok_or("missing")?]);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CybersecurityGoal {
    pub(crate) name: Arc<str>,
    /// Sorted by the target's `ControlId`, unique, at most [`MAX_GOAL_CONTROLS`].
    pub(crate) controls: Vec<ControlIdx>,
    /// Sorted by `PartyId`, unique, at most 16 parties.
    pub(crate) responsible: Vec<PartyIdx>,
    pub(crate) provenance: Provenance,
}

impl CybersecurityGoal {
    /// Returns the cybersecurity goal's name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id,
    ///     name: "Ensure firmware authenticity".into(),
    ///     controls: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// let g = project.cybersecurity_goal(idx).ok_or("missing")?;
    /// assert_eq!(g.name(), "Ensure firmware authenticity");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the linked security controls, sorted by public ID and unique.
    /// Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ControlId, GoalId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let ctrl = ControlId::new_v7();
    /// project.apply(Op::AddSecurityControl {
    ///     id: ctrl,
    ///     name: "Secure boot".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let id = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id,
    ///     name: "Ensure firmware authenticity".into(),
    ///     controls: vec![ctrl],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// let g = project.cybersecurity_goal(idx).ok_or("missing")?;
    /// assert_eq!(g.controls(), [project.security_control_idx(&ctrl).ok_or("missing")?]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn controls(&self) -> &[ControlIdx] {
        &self.controls
    }

    /// Returns the parties responsible for this goal, sorted by
    /// [`PartyId`](crate::PartyId) and unique. Time O(1).
    #[must_use]
    pub fn responsible(&self) -> &[PartyIdx] {
        &self.responsible
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this cybersecurity goal. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{GoalId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = GoalId::new_v7();
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id,
    ///     name: "Ensure firmware authenticity".into(),
    ///     controls: vec![],
    ///     provenance: prov.clone(),
    /// })?;
    /// let idx = project.cybersecurity_goal_idx(&id).ok_or("missing")?;
    /// let g = project.cybersecurity_goal(idx).ok_or("missing")?;
    /// assert_eq!(g.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// A cybersecurity claim in a [`Project`](crate::Project) (ISO/SAE 21434
/// Clause 15.5).
///
/// A cybersecurity claim records the rationale and assumptions justifying a
/// [`RiskTreatment::Retain`] or [`RiskTreatment::Share`] decision on one or
/// more threat scenarios.
///
/// Built and changed only through [`Project::apply`](crate::Project::apply).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let id = ClaimId::new_v7();
/// project.apply(Op::AddCybersecurityClaim {
///     id,
///     name: "Backend PKI is operated by certified TSP under SLA".into(),
///     provenance: Provenance::default(),
/// })?;
/// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
/// let cl = project.cybersecurity_claim(idx).ok_or("missing")?;
/// assert_eq!(cl.name(), "Backend PKI is operated by certified TSP under SLA");
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CybersecurityClaim {
    pub(crate) name: Arc<str>,
    /// Sorted by `PartyId`, unique, at most 16 parties.
    pub(crate) responsible: Vec<PartyIdx>,
    pub(crate) provenance: Provenance,
}

impl CybersecurityClaim {
    /// Returns the cybersecurity claim's statement/name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ClaimId::new_v7();
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id,
    ///     name: "Physical access requires locked enclosure key".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
    /// let cl = project.cybersecurity_claim(idx).ok_or("missing")?;
    /// assert_eq!(cl.name(), "Physical access requires locked enclosure key");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parties responsible for this claim, sorted by
    /// [`PartyId`](crate::PartyId) and unique. Time O(1).
    #[must_use]
    pub fn responsible(&self) -> &[PartyIdx] {
        &self.responsible
    }

    /// Returns the provenance of the last [`Op`](crate::Op) that created or
    /// modified this cybersecurity claim. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{ClaimId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = ClaimId::new_v7();
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id,
    ///     name: "Physical access requires locked enclosure key".into(),
    ///     provenance: prov.clone(),
    /// })?;
    /// let idx = project.cybersecurity_claim_idx(&id).ok_or("missing")?;
    /// let cl = project.cybersecurity_claim(idx).ok_or("missing")?;
    /// assert_eq!(cl.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}
