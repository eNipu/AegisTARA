//! Tipping-point sensitivity analysis for attack trees and threat scenarios.
//!
//! A **tipping point** is a single input rating where a $\pm 1$ step on its
//! methodology scale changes the evaluated conclusion: the root
//! [`FeasibilityLevel`] of an [`AttackTree`](crate::AttackTree)
//! ([`StepTippingPoint`]) or the worst-case [`RiskLevel`] of a
//! [`ThreatScenario`](crate::ThreatScenario) ([`ThreatSensitivity`],
//! [`FeasibilityTippingPoint`], [`ImpactTippingPoint`]).
//!
//! All types hold only typed IDs, arena indices, and methodology scale
//! levels, never user-supplied names or labels (rule R6).

use crate::id::{DamageId, NodeId, NodeIdx};
use crate::methodology::{FeasibilityLevel, ImpactCategory, Level, RiskLevel};

/// One leaf step where a $\pm 1$ move on the methodology's feasibility scale
/// changes the root feasibility of an [`AttackTree`](crate::AttackTree),
/// produced by [`Project::tree_tipping_points`](crate::Project::tree_tipping_points)
/// and [`Project::residual_tree_tipping_points`](crate::Project::residual_tree_tipping_points).
///
/// Holds only [`NodeIdx`], [`NodeId`], and [`FeasibilityLevel`] values, never
/// user-supplied labels (rule R6).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AttackTree, GateKind, Methodology, Node, NodeId, Op, Project, Provenance, TreeId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let (low, med, high) = (
///     m.feasibility_level("low").ok_or("no level")?,
///     m.feasibility_level("medium").ok_or("no level")?,
///     m.feasibility_level("high").ok_or("no level")?,
/// );
/// let (root, s1, s2) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
/// let mut tree = AttackTree::new(root, Node::gate(GateKind::And, "Goal"))?;
/// let i1 = tree.add_child(tree.root(), s1, Node::step("Bottleneck"))?;
/// tree.add_child(tree.root(), s2, Node::step("Easy"))?;
/// let tree_id = TreeId::new_v7();
/// project.apply(Op::AddAttackTree {
///     role: Default::default(),
///     id: tree_id,
///     tree,
///     step_feasibility: vec![(s1, med), (s2, high)],
///     provenance: Provenance::default(),
/// })?;
/// let tips = project.tree_tipping_points(tree_id)?;
/// assert_eq!(tips.len(), 2);
/// assert_eq!(tips[0].step(), i1);
/// assert_eq!(tips[0].node(), s1);
/// assert_eq!((tips[0].from_step(), tips[0].to_step()), (med, low));
/// assert_eq!((tips[0].from_root(), tips[0].to_root()), (med, low));
/// assert_eq!((tips[1].from_step(), tips[1].to_step()), (med, high));
/// assert_eq!((tips[1].from_root(), tips[1].to_root()), (med, high));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepTippingPoint {
    pub(crate) step: NodeIdx,
    pub(crate) node: NodeId,
    pub(crate) from_step: FeasibilityLevel,
    pub(crate) to_step: FeasibilityLevel,
    pub(crate) from_root: FeasibilityLevel,
    pub(crate) to_root: FeasibilityLevel,
}

impl StepTippingPoint {
    /// Returns the arena index of the perturbed leaf step. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let step = NodeId::new_v7();
    /// let tree = AttackTree::new(step, Node::step("Only"))?;
    /// let root_idx = tree.root();
    /// let tree_id = TreeId::new_v7();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     role: Default::default(),
    ///     id: tree_id, tree, step_feasibility: vec![(step, med)],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// assert_eq!(tips[0].step(), root_idx);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn step(&self) -> NodeIdx {
        self.step
    }

    /// Returns the stable `UUIDv7` of the perturbed leaf step. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     role: Default::default(),
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Only"))?,
    ///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// assert_eq!(tips[0].node(), step);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn node(&self) -> NodeId {
        self.node
    }

    /// Returns the step's baseline feasibility level before the perturbation.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     role: Default::default(),
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Only"))?,
    ///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// assert_eq!(tips[0].from_step(), med);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_step(&self) -> FeasibilityLevel {
        self.from_step
    }

    /// Returns the step's perturbed feasibility level ($\pm 1$ scale step from
    /// [`from_step`](Self::from_step)). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let (low, med) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    /// );
    /// project.apply(Op::AddAttackTree {
    ///     role: Default::default(),
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Only"))?,
    ///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// assert_eq!(tips[0].to_step(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_step(&self) -> FeasibilityLevel {
        self.to_step
    }

    /// Returns the tree's baseline root feasibility level. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::AddAttackTree {
    ///     role: Default::default(),
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Only"))?,
    ///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// assert_eq!(tips[0].from_root(), med);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_root(&self) -> FeasibilityLevel {
        self.from_root
    }

    /// Returns the tree's root feasibility level after the perturbation
    /// (always distinct from [`from_root`](Self::from_root)). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, TreeId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let step = NodeId::new_v7();
    /// let tree_id = TreeId::new_v7();
    /// let (low, med) = (
    ///     m.feasibility_level("low").ok_or("no level")?,
    ///     m.feasibility_level("medium").ok_or("no level")?,
    /// );
    /// project.apply(Op::AddAttackTree {
    ///     role: Default::default(),
    ///     id: tree_id, tree: AttackTree::new(step, Node::step("Only"))?,
    ///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
    /// })?;
    /// let tips = project.tree_tipping_points(tree_id)?;
    /// assert_eq!(tips[0].to_root(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_root(&self) -> FeasibilityLevel {
        self.to_root
    }
}

/// One $\pm 1$ feasibility perturbation (on a leaf step of a linked attack
/// tree, or on a threat's direct feasibility when no tree is linked) that
/// changes a threat scenario's worst-case [`RiskLevel`].
///
/// Holds only [`NodeIdx`], [`NodeId`], [`FeasibilityLevel`], and [`RiskLevel`]
/// values, never user-supplied labels or names (rule R6).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let asset = AssetId::new_v7();
/// project.apply(Op::AddAsset {
///     id: asset, name: "ECU".into(), properties: m.all_properties(),
///     provenance: Provenance::default(),
/// })?;
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let severe = m.level(safety, "severe").ok_or("no level")?;
/// let damage = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id: damage, name: "Crash".into(), impacts: vec![(safety, severe)],
///     provenance: Provenance::default(),
/// })?;
/// let category = m.threat_categories().next().ok_or("none")?;
/// let threat = ThreatId::new_v7();
/// project.apply(Op::AddThreatScenario {
///     id: threat, name: "Spoof".into(), asset, category,
///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
/// })?;
/// let med = m.feasibility_level("medium").ok_or("no level")?;
/// project.apply(Op::SetThreatFeasibility {
///     threat, level: Some(med), provenance: Provenance::default(),
/// })?;
/// let sens = project.threat_sensitivity(threat)?;
/// let ft = &sens.feasibility_tipping_points()[0];
/// assert_eq!(ft.step(), None);
/// assert_eq!(ft.from_risk(), m.risk_level("r4").ok_or("no r4")?);
/// assert_eq!(ft.to_risk(), m.risk_level("r3").ok_or("no r3")?);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeasibilityTippingPoint {
    pub(crate) step: Option<NodeIdx>,
    pub(crate) node: Option<NodeId>,
    pub(crate) from_step_feasibility: FeasibilityLevel,
    pub(crate) to_step_feasibility: FeasibilityLevel,
    pub(crate) from_threat_feasibility: FeasibilityLevel,
    pub(crate) to_threat_feasibility: FeasibilityLevel,
    pub(crate) from_risk: RiskLevel,
    pub(crate) to_risk: RiskLevel,
}

impl FeasibilityTippingPoint {
    /// Returns the arena index of the perturbed leaf step when the threat is
    /// realized by a linked attack tree, or `None` when the threat's direct
    /// feasibility was perturbed. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].step(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn step(&self) -> Option<NodeIdx> {
        self.step
    }

    /// Returns the stable `UUIDv7` of the perturbed leaf step when the threat is
    /// realized by a linked attack tree, or `None` when the threat's direct
    /// feasibility was perturbed. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].node(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn node(&self) -> Option<NodeId> {
        self.node
    }

    /// Returns the baseline feasibility of the perturbed step (or direct threat
    /// rating). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].from_step_feasibility(), med);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_step_feasibility(&self) -> FeasibilityLevel {
        self.from_step_feasibility
    }

    /// Returns the perturbed ($\pm 1$) feasibility of the step (or direct
    /// threat rating). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let (low, med) = (m.feasibility_level("low").ok_or("no level")?, m.feasibility_level("medium").ok_or("no level")?);
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].to_step_feasibility(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_step_feasibility(&self) -> FeasibilityLevel {
        self.to_step_feasibility
    }

    /// Returns the threat's baseline overall feasibility level. Time O(1),
    /// space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].from_threat_feasibility(), med);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_threat_feasibility(&self) -> FeasibilityLevel {
        self.from_threat_feasibility
    }

    /// Returns the threat's overall feasibility level after the perturbation.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let (low, med) = (m.feasibility_level("low").ok_or("no level")?, m.feasibility_level("medium").ok_or("no level")?);
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].to_threat_feasibility(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_threat_feasibility(&self) -> FeasibilityLevel {
        self.to_threat_feasibility
    }

    /// Returns the threat's baseline worst-case risk level. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].from_risk(), m.risk_level("r4").ok_or("no r4")?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_risk(&self) -> RiskLevel {
        self.from_risk
    }

    /// Returns the threat's worst-case risk level after the perturbation
    /// (always distinct from [`from_risk`](Self::from_risk)). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.feasibility_tipping_points()[0].to_risk(), m.risk_level("r3").ok_or("no r3")?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_risk(&self) -> RiskLevel {
        self.to_risk
    }
}

/// One $\pm 1$ impact rating perturbation on a linked damage scenario that
/// changes a threat scenario's worst-case [`RiskLevel`].
///
/// Holds only [`DamageId`], [`ImpactCategory`], [`Level`], and [`RiskLevel`]
/// values, never user-supplied names (rule R6).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let asset = AssetId::new_v7();
/// project.apply(Op::AddAsset {
///     id: asset, name: "ECU".into(), properties: m.all_properties(),
///     provenance: Provenance::default(),
/// })?;
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let (major, severe) = (
///     m.level(safety, "major").ok_or("no level")?,
///     m.level(safety, "severe").ok_or("no level")?,
/// );
/// let damage = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id: damage, name: "Crash".into(), impacts: vec![(safety, severe)],
///     provenance: Provenance::default(),
/// })?;
/// let category = m.threat_categories().next().ok_or("none")?;
/// let threat = ThreatId::new_v7();
/// project.apply(Op::AddThreatScenario {
///     id: threat, name: "Spoof".into(), asset, category,
///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
/// })?;
/// let high = m.feasibility_level("high").ok_or("no level")?;
/// project.apply(Op::SetThreatFeasibility {
///     threat, level: Some(high), provenance: Provenance::default(),
/// })?;
/// let sens = project.threat_sensitivity(threat)?;
/// let it = &sens.impact_tipping_points()[0];
/// assert_eq!(it.damage(), damage);
/// assert_eq!(it.category(), safety);
/// assert_eq!((it.from_impact(), it.to_impact()), (severe, major));
/// assert_eq!(it.from_risk(), m.risk_level("r5").ok_or("no r5")?);
/// assert_eq!(it.to_risk(), m.risk_level("r4").ok_or("no r4")?);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpactTippingPoint {
    pub(crate) damage: DamageId,
    pub(crate) category: ImpactCategory,
    pub(crate) from_impact: Level,
    pub(crate) to_impact: Level,
    pub(crate) from_risk: RiskLevel,
    pub(crate) to_risk: RiskLevel,
}

impl ImpactTippingPoint {
    /// Returns the linked damage scenario whose impact rating was perturbed.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(high), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.impact_tipping_points()[0].damage(), damage);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn damage(&self) -> DamageId {
        self.damage
    }

    /// Returns the impact category that was perturbed. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(high), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.impact_tipping_points()[0].category(), safety);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn category(&self) -> ImpactCategory {
        self.category
    }

    /// Returns the baseline impact level on `(damage, category)`. Time O(1),
    /// space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(high), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.impact_tipping_points()[0].from_impact(), severe);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_impact(&self) -> Level {
        self.from_impact
    }

    /// Returns the perturbed ($\pm 1$) impact level on `(damage, category)`.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let (major, severe) = (m.level(safety, "major").ok_or("no level")?, m.level(safety, "severe").ok_or("no level")?);
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(high), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.impact_tipping_points()[0].to_impact(), major);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_impact(&self) -> Level {
        self.to_impact
    }

    /// Returns the threat's baseline worst-case risk level. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(high), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.impact_tipping_points()[0].from_risk(), m.risk_level("r5").ok_or("no r5")?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn from_risk(&self) -> RiskLevel {
        self.from_risk
    }

    /// Returns the threat's worst-case risk level after the perturbation
    /// (always distinct from [`from_risk`](Self::from_risk)). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(high), provenance: Provenance::default() })?;
    /// let sens = project.threat_sensitivity(threat)?;
    /// assert_eq!(sens.impact_tipping_points()[0].to_risk(), m.risk_level("r4").ok_or("no r4")?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn to_risk(&self) -> RiskLevel {
        self.to_risk
    }
}

/// Complete single-step ($\pm 1$) sensitivity analysis of a
/// [`ThreatScenario`](crate::ThreatScenario), produced by
/// [`Project::threat_sensitivity`](crate::Project::threat_sensitivity) and
/// [`Project::residual_threat_sensitivity`](crate::Project::residual_threat_sensitivity).
///
/// Reports the threat's baseline feasibility and worst-case risk alongside
/// every single-step ($\pm 1$) perturbation to a leaf step (or direct threat
/// feasibility) or to a linked damage impact rating that shifts the threat's
/// worst-case [`RiskLevel`].
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{
///     AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId,
/// };
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let asset = AssetId::new_v7();
/// project.apply(Op::AddAsset {
///     id: asset, name: "ECU".into(), properties: m.all_properties(),
///     provenance: Provenance::default(),
/// })?;
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let negligible = m.level(safety, "negligible").ok_or("no level")?;
/// let damage = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id: damage, name: "Scratch".into(), impacts: vec![(safety, negligible)],
///     provenance: Provenance::default(),
/// })?;
/// let category = m.threat_categories().next().ok_or("none")?;
/// let threat = ThreatId::new_v7();
/// project.apply(Op::AddThreatScenario {
///     id: threat, name: "Spoof".into(), asset, category,
///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
/// })?;
/// let low = m.feasibility_level("low").ok_or("no level")?;
/// project.apply(Op::SetThreatFeasibility {
///     threat, level: Some(low), provenance: Provenance::default(),
/// })?;
/// // At (negligible, low), every +-1 move in feasibility or impact stays at r1.
/// let sens = project.threat_sensitivity(threat)?;
/// assert!(sens.is_stable());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatSensitivity {
    pub(crate) feasibility: FeasibilityLevel,
    pub(crate) max_risk: Option<RiskLevel>,
    pub(crate) feasibility_tipping_points: Vec<FeasibilityTippingPoint>,
    pub(crate) impact_tipping_points: Vec<ImpactTippingPoint>,
}

impl ThreatSensitivity {
    /// Returns the threat's baseline feasibility level. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// assert_eq!(project.threat_sensitivity(threat)?.feasibility(), med);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn feasibility(&self) -> FeasibilityLevel {
        self.feasibility
    }

    /// Returns the threat's baseline worst-case risk level across all linked
    /// damage scenarios, or `None` if no linked damage has a rated impact.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// assert_eq!(project.threat_sensitivity(threat)?.max_risk(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn max_risk(&self) -> Option<RiskLevel> {
        self.max_risk
    }

    /// Returns all single-step ($\pm 1$) feasibility perturbations that change
    /// [`max_risk`](Self::max_risk), ordered by step index ascending (with
    /// $-1$ before $+1$). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// assert!(project.threat_sensitivity(threat)?.feasibility_tipping_points().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn feasibility_tipping_points(&self) -> &[FeasibilityTippingPoint] {
        &self.feasibility_tipping_points
    }

    /// Returns all single-step ($\pm 1$) impact perturbations on linked damage
    /// scenarios that change [`max_risk`](Self::max_risk), ordered by
    /// `(DamageId, ImpactCategory)` ascending (with $-1$ before $+1$).
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// assert!(project.threat_sensitivity(threat)?.impact_tipping_points().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn impact_tipping_points(&self) -> &[ImpactTippingPoint] {
        &self.impact_tipping_points
    }

    /// Returns `true` when neither a single $\pm 1$ feasibility perturbation
    /// nor a single $\pm 1$ impact perturbation changes
    /// [`max_risk`](Self::max_risk). Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let med = m.feasibility_level("medium").ok_or("no level")?;
    /// # project.apply(Op::SetThreatFeasibility { threat, level: Some(med), provenance: Provenance::default() })?;
    /// assert!(project.threat_sensitivity(threat)?.is_stable());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_stable(&self) -> bool {
        self.feasibility_tipping_points.is_empty() && self.impact_tipping_points.is_empty()
    }
}
