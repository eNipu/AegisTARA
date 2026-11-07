//! Threat scenarios: how an asset's properties can be violated.

use std::sync::Arc;

use crate::control::RiskTreatment;
use crate::eval::Evaluation;
use crate::id::{AssetIdx, ClaimIdx, DamageId, DamageIdx, GoalIdx, TreeIdx};
use crate::methodology::{
    CatalogRef, FeasibilityLevel, ImpactCategory, Level, RiskLevel, ThreatCategory,
};
use crate::provenance::Provenance;

/// Most catalog refs on one threat scenario.
pub const MAX_CATALOG_REFS: usize = 16;

/// Most damage scenarios one threat scenario may link to.
///
/// Bounds the size of one op and one saved threat, so the save/load
/// decoder can cap every list at decode time and still accept every
/// project the model accepts (see `docs/adr/0009-save-load-format.md`).
///
/// ```
/// assert_eq!(aegis_model::MAX_DAMAGE_LINKS, 256);
/// ```
pub const MAX_DAMAGE_LINKS: usize = 256;

/// A threat scenario of a [`Project`](crate::Project).
///
/// A read-only view. It targets one asset, has one threat category (the
/// targeted properties are derived from it through
/// [`Project::targets`](crate::Project::targets), so they can never
/// disagree), cites 0 to 16 catalog refs, and links to damage scenarios.
/// `catalog_refs` is sorted and unique; `damages` is sorted by the damage
/// scenario's public ID, unique, and at most [`MAX_DAMAGE_LINKS`] long.
/// Neither order depends on edit history.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let asset = AssetId::new_v7();
/// project.apply(Op::AddAsset {
///     id: asset,
///     name: "ECU".into(),
///     properties: m.all_properties(),
///     provenance: Provenance::default(),
/// })?;
/// let category = m.threat_category("tampering").ok_or("no category")?;
/// project.apply(Op::AddThreatScenario {
///     id: ThreatId::new_v7(),
///     name: "Flash malicious firmware".into(),
///     asset,
///     category,
///     catalog_refs: vec![],
///     damages: vec![],
///     provenance: Provenance::default(),
/// })?;
/// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
/// assert_eq!(t.category(), category);
/// assert_eq!(Some(t.asset()), project.asset_idx(&asset));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatScenario {
    pub(crate) name: Arc<str>,
    pub(crate) asset: AssetIdx,
    pub(crate) category: ThreatCategory,
    /// Sorted, unique, at most [`MAX_CATALOG_REFS`].
    pub(crate) catalog_refs: Vec<CatalogRef>,
    /// Sorted by the target's `DamageId`, unique, at most
    /// [`MAX_DAMAGE_LINKS`].
    pub(crate) damages: Vec<DamageIdx>,
    pub(crate) direct_feasibility: Option<FeasibilityLevel>,
    pub(crate) attack_tree: Option<TreeIdx>,
    pub(crate) treatment: Option<RiskTreatment>,
    pub(crate) goal: Option<GoalIdx>,
    pub(crate) claim: Option<ClaimIdx>,
    /// Whether the attack scales to many vehicles (FND-10); `None` is not
    /// rated.
    pub(crate) scales: Option<bool>,
    /// Why it scales or not; empty when not given.
    pub(crate) scaling_reason: Arc<str>,
    pub(crate) provenance: Provenance,
}

impl ThreatScenario {
    /// Returns the name. Time O(1).
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
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.name(), "Replay");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the targeted asset. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(project.asset_id(t.asset()), Some(asset));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset(&self) -> AssetIdx {
        self.asset
    }

    /// Returns the threat category. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// let category = m.threat_category("spoofing").ok_or("none")?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Fake key fob".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.category(), category);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn category(&self) -> ThreatCategory {
        self.category
    }

    /// Returns the catalog refs, sorted and unique. Time O(1).
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
    /// let scheme = m.catalog_scheme("r155-annex5").ok_or("none")?;
    /// let (a, b) = (m.parse_catalog_ref(scheme, "5")?, m.parse_catalog_ref(scheme, "4.1")?);
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![a, b], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.catalog_refs(), [b, a]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_refs(&self) -> &[CatalogRef] {
        &self.catalog_refs
    }

    /// Returns the linked damage scenarios, sorted by public ID and unique.
    /// Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// let damage = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![], provenance: Provenance::default() })?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.damages(), [project.damage_scenario_idx(&damage).ok_or("missing")?]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damages(&self) -> &[DamageIdx] {
        &self.damages
    }

    /// Returns the direct feasibility rating assigned to this threat
    /// scenario (via [`Op::SetThreatFeasibility`](crate::Op::SetThreatFeasibility)),
    /// if any. Time O(1), space O(1).
    ///
    /// When [`attack_tree`](Self::attack_tree) is also `Some`,
    /// [`Project::evaluate_risk`](crate::Project::evaluate_risk) uses the
    /// linked attack tree's evaluated feasibility while keeping this direct
    /// rating intact on the threat scenario.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, Provenance, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let medium = m.feasibility_level("medium").ok_or("no level")?;
    /// project.apply(Op::SetThreatFeasibility {
    ///     threat, level: Some(medium), provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.direct_feasibility(), Some(medium));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn direct_feasibility(&self) -> Option<FeasibilityLevel> {
        self.direct_feasibility
    }

    /// Returns the linked attack tree's arena index, if any. Time O(1),
    /// space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let tree_id = TreeId::new_v7();
    /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("Step"))?;
    /// project.apply(Op::AddAttackTree {
    ///     id: tree_id, tree, step_feasibility: vec![], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatAttackTree {
    ///     threat, tree: Some(tree_id), provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.attack_tree(), project.attack_tree_idx(&tree_id));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn attack_tree(&self) -> Option<TreeIdx> {
        self.attack_tree
    }

    /// Returns the risk treatment decision assigned to this threat scenario
    /// (via [`Op::SetRiskTreatment`](crate::Op::SetRiskTreatment)), if any.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, Methodology, Op, Project, Provenance, RiskTreatment, ThreatId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetRiskTreatment {
    ///     threat,
    ///     treatment: Some(RiskTreatment::Reduce),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.treatment(), Some(RiskTreatment::Reduce));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn treatment(&self) -> Option<RiskTreatment> {
        self.treatment
    }

    /// Returns the linked [`CybersecurityGoal`](crate::CybersecurityGoal)'s
    /// arena index (assigned via [`Op::SetThreatGoal`](crate::Op::SetThreatGoal)),
    /// if any. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, GoalId, Methodology, Op, Project, Provenance, ThreatId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let goal = GoalId::new_v7();
    /// project.apply(Op::AddCybersecurityGoal {
    ///     id: goal, name: "Authenticate messages".into(), controls: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatGoal {
    ///     threat, goal: Some(goal), provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.goal(), project.cybersecurity_goal_idx(&goal));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn goal(&self) -> Option<GoalIdx> {
        self.goal
    }

    /// Returns the linked [`CybersecurityClaim`](crate::CybersecurityClaim)'s
    /// arena index (assigned via [`Op::SetThreatClaim`](crate::Op::SetThreatClaim)),
    /// if any. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, ClaimId, Methodology, Op, Project, Provenance, ThreatId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id: asset, name: "ECU".into(), properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let category = m.threat_categories().next().ok_or("none")?;
    /// let threat = ThreatId::new_v7();
    /// project.apply(Op::AddThreatScenario {
    ///     id: threat, name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: Provenance::default(),
    /// })?;
    /// let claim = ClaimId::new_v7();
    /// project.apply(Op::AddCybersecurityClaim {
    ///     id: claim, name: "Diagnostic bus disabled in field".into(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// project.apply(Op::SetThreatClaim {
    ///     threat, claim: Some(claim), provenance: Provenance::default(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.claim(), project.cybersecurity_claim_idx(&claim));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn claim(&self) -> Option<ClaimIdx> {
        self.claim
    }

    /// Whether the attack scales to many vehicles (FND-10), set with
    /// [`Op::SetThreatScaling`](crate::Op::SetThreatScaling); `None` is not
    /// rated. Time O(1).
    #[must_use]
    pub const fn scales(&self) -> Option<bool> {
        self.scales
    }

    /// The reasoning for [`scales`](Self::scales), empty when not given.
    /// Time O(1).
    #[must_use]
    pub fn scaling_reason(&self) -> &str {
        &self.scaling_reason
    }

    /// Returns the provenance of the last creation or modification of this
    /// threat scenario. Time O(1), space O(1).
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
    /// let prov = Provenance::catalog("r155-annex5", 1_700_000_000_000);
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![], provenance: prov.clone(),
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

/// One cell of a threat scenario's risk determination: the [`RiskLevel`]
/// produced by looking up `(impact, feasibility)` in `category`'s 2D risk
/// matrix for a linked damage scenario.
///
/// Returned inside [`ThreatRisk::entries`] and [`ThreatRisk::deciding_entry`].
///
/// ```
/// # use std::sync::Arc;
/// # use aegis_model::{
/// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
/// #     ThreatId, TreeId,
/// # };
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
/// let tree_id = TreeId::new_v7();
/// let step = NodeId::new_v7();
/// let high = m.feasibility_level("high").ok_or("no level")?;
/// let tree = AttackTree::new(step, Node::step("Inject"))?;
/// project.apply(Op::AddAttackTree {
///     id: tree_id, tree, step_feasibility: vec![(step, high)],
///     provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetThreatAttackTree {
///     threat, tree: Some(tree_id), provenance: Provenance::default(),
/// })?;
/// let eval = project.evaluate_risk(threat)?;
/// let entry = eval.deciding_entry().ok_or("no entry")?;
/// assert_eq!(entry.damage(), damage);
/// assert_eq!(entry.category(), safety);
/// assert_eq!(entry.impact(), severe);
/// assert_eq!(entry.feasibility(), high);
/// assert_eq!(entry.risk(), m.risk_level("r5").ok_or("no risk")?);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImpactRisk {
    pub(crate) damage: DamageId,
    pub(crate) category: ImpactCategory,
    pub(crate) impact: Level,
    pub(crate) feasibility: FeasibilityLevel,
    pub(crate) risk: RiskLevel,
}

impl ImpactRisk {
    /// Returns the damage scenario's public ID. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, high)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let eval = project.evaluate_risk(threat)?;
    /// assert_eq!(eval.entries()[0].damage(), damage);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn damage(&self) -> DamageId {
        self.damage
    }

    /// Returns the impact category. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// let safety = m.impact_category("safety").ok_or("no category")?;
    /// # let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, high)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let eval = project.evaluate_risk(threat)?;
    /// assert_eq!(eval.entries()[0].category(), safety);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn category(&self) -> ImpactCategory {
        self.category
    }

    /// Returns the impact rating in `category`'s scale. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let safety = m.impact_category("safety").ok_or("no category")?;
    /// let severe = m.level(safety, "severe").ok_or("no level")?;
    /// # let damage = DamageId::new_v7();
    /// # project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![(safety, severe)], provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, high)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let eval = project.evaluate_risk(threat)?;
    /// assert_eq!(eval.entries()[0].impact(), severe);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn impact(&self) -> Level {
        self.impact
    }

    /// Returns the threat's feasibility level used in the matrix lookup. Time
    /// O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
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
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, high)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let eval = project.evaluate_risk(threat)?;
    /// assert_eq!(eval.entries()[0].feasibility(), high);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn feasibility(&self) -> FeasibilityLevel {
        self.feasibility
    }

    /// Returns the resulting risk level in the methodology's risk scale. Time
    /// O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
    /// #     ThreatId, TreeId,
    /// # };
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
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let high = m.feasibility_level("high").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, high)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let eval = project.evaluate_risk(threat)?;
    /// assert_eq!(eval.entries()[0].risk(), m.risk_level("r5").ok_or("no risk")?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn risk(&self) -> RiskLevel {
        self.risk
    }
}

/// Risk determination for a threat scenario across all of its linked damage
/// scenarios and their rated impact categories.
///
/// Returned by [`Project::evaluate_risk`](crate::Project::evaluate_risk).
/// Entries are ordered deterministically by `(DamageId, ImpactCategory)`,
/// and [`deciding_entry`](Self::deciding_entry) points to the first entry in
/// that order that attains [`max_risk`](Self::max_risk).
///
/// ```
/// # use std::sync::Arc;
/// # use aegis_model::{
/// #     AssetId, AttackTree, DamageId, Methodology, Node, NodeId, Op, Project, Provenance,
/// #     ThreatId, TreeId,
/// # };
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let asset = AssetId::new_v7();
/// project.apply(Op::AddAsset {
///     id: asset, name: "ECU".into(), properties: m.all_properties(),
///     provenance: Provenance::default(),
/// })?;
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let financial = m.impact_category("financial").ok_or("no category")?;
/// let severe = m.level(safety, "severe").ok_or("no level")?;
/// let minor = m.level(financial, "moderate").ok_or("no level")?;
/// let damage = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id: damage, name: "Crash".into(), impacts: vec![(safety, severe), (financial, minor)],
///     provenance: Provenance::default(),
/// })?;
/// let category = m.threat_categories().next().ok_or("none")?;
/// let threat = ThreatId::new_v7();
/// project.apply(Op::AddThreatScenario {
///     id: threat, name: "Spoof".into(), asset, category,
///     catalog_refs: vec![], damages: vec![damage], provenance: Provenance::default(),
/// })?;
/// let tree_id = TreeId::new_v7();
/// let step = NodeId::new_v7();
/// let med = m.feasibility_level("medium").ok_or("no level")?;
/// project.apply(Op::AddAttackTree {
///     id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?,
///     step_feasibility: vec![(step, med)], provenance: Provenance::default(),
/// })?;
/// project.apply(Op::SetThreatAttackTree {
///     threat, tree: Some(tree_id), provenance: Provenance::default(),
/// })?;
/// let risk = project.evaluate_risk(threat)?;
/// assert_eq!(risk.feasibility(), med);
/// assert_eq!(risk.entries().len(), 2);
/// assert_eq!(risk.max_risk(), m.risk_level("r4"));
/// assert_eq!(risk.deciding_entry().map(|e| e.category()), Some(safety));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatRisk {
    pub(crate) feasibility: FeasibilityLevel,
    pub(crate) tree_evaluation: Option<Evaluation<FeasibilityLevel>>,
    pub(crate) entries: Vec<ImpactRisk>,
    pub(crate) deciding: Option<usize>,
}

impl ThreatRisk {
    /// Returns the threat scenario's evaluated feasibility level. Time O(1),
    /// space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, low)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// assert_eq!(project.evaluate_risk(threat)?.feasibility(), low);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn feasibility(&self) -> FeasibilityLevel {
        self.feasibility
    }

    /// Returns the underlying attack tree evaluation if the threat's
    /// feasibility came from an attack tree, or `None` if it came from a
    /// direct rating on the threat scenario. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let low = m.feasibility_level("low").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, low)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let risk = project.evaluate_risk(threat)?;
    /// assert_eq!(risk.tree_evaluation().map(|e| *e.root_value()), Some(low));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn tree_evaluation(&self) -> Option<&Evaluation<FeasibilityLevel>> {
        self.tree_evaluation.as_ref()
    }

    /// Returns all per-damage, per-category risk entries in canonical
    /// `(DamageId, ImpactCategory)` order. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let low = m.feasibility_level("low").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, low)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// let risk = project.evaluate_risk(threat)?;
    /// assert!(risk.entries().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn entries(&self) -> &[ImpactRisk] {
        &self.entries
    }

    /// Returns the worst-case (highest) risk level across all entries, or
    /// `None` if no linked damage scenario has any rated impact category.
    /// Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let low = m.feasibility_level("low").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, low)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// assert_eq!(project.evaluate_risk(threat)?.max_risk(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn max_risk(&self) -> Option<RiskLevel> {
        self.deciding_entry().map(ImpactRisk::risk)
    }

    /// Returns the first entry in canonical `(DamageId, ImpactCategory)`
    /// order that attains [`max_risk`](Self::max_risk), or `None` if
    /// [`entries`](Self::entries) is empty. Time O(1), space O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{
    /// #     AssetId, AttackTree, Methodology, Node, NodeId, Op, Project, Provenance, ThreatId,
    /// #     TreeId,
    /// # };
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties(), provenance: Provenance::default() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// # let threat = ThreatId::new_v7();
    /// # project.apply(Op::AddThreatScenario { id: threat, name: "Spoof".into(), asset, category, catalog_refs: vec![], damages: vec![], provenance: Provenance::default() })?;
    /// # let tree_id = TreeId::new_v7();
    /// # let step = NodeId::new_v7();
    /// # let low = m.feasibility_level("low").ok_or("no level")?;
    /// # project.apply(Op::AddAttackTree { id: tree_id, tree: AttackTree::new(step, Node::step("Inject"))?, step_feasibility: vec![(step, low)], provenance: Provenance::default() })?;
    /// # project.apply(Op::SetThreatAttackTree { threat, tree: Some(tree_id), provenance: Provenance::default() })?;
    /// assert_eq!(project.evaluate_risk(threat)?.deciding_entry(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn deciding_entry(&self) -> Option<&ImpactRisk> {
        self.deciding.and_then(|i| self.entries.get(i))
    }
}
