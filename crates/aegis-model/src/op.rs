//! Typed operations on a [`Project`](crate::Project).

use std::sync::Arc;

use crate::asset::PropertySet;
use crate::control::RiskTreatment;
use crate::description::ElementRef;
use crate::id::{AssetId, ClaimId, ControlId, DamageId, GoalId, NodeId, ThreatId, TreeId};
use crate::methodology::{
    CatalogRef, FactorLevel, FeasibilityFactor, FeasibilityLevel, ImpactCategory, Level,
    ThreatCategory,
};
use crate::provenance::Provenance;
use crate::tree::AttackTree;

/// One change to a [`Project`](crate::Project).
///
/// Entities are named by public ID; vocabulary by the typed indices of the
/// project's methodology. Every creating or mutating variant carries a
/// [`Provenance`] recording who made the change, when, and from what source
/// (D6). [`Project::apply`](crate::Project::apply) checks the op, commits
/// it, and returns its inverse (carrying the element's prior
/// [`Provenance`]), so undo is "apply the inverse", restores both domain
/// state and provenance, and costs O(size of the op). Costs below use
/// n = entities of the kind, k = damage links or catalog refs on the
/// threat, p <= 64 properties, c <= 32 impact categories, V = nodes in the
/// attack tree, E = edges in the attack tree.
///
/// | Op | Inverse | Cost |
/// |---|---|---|
/// | `SetDescription` | the same with the old text and provenance (empty text clears) | O(log n + text) |
/// | `AddAsset` | `RemoveAsset` | O(log n) |
/// | `RenameAsset` | `RenameAsset` with the old name and provenance | O(log n) |
/// | `SetAssetProperties` | the same with the old set and provenance | O(log n + p) |
/// | `RemoveAsset` | `AddAsset` with the old fields and provenance | O(log n + p) |
/// | `AddDamageScenario` | `RemoveDamageScenario` | O(log n + c) |
/// | `RenameDamageScenario` | the same with the old name and provenance | O(log n) |
/// | `SetImpact` | the same with the old level and provenance | O(log n) |
/// | `RemoveDamageScenario` | `AddDamageScenario`, rated pairs in category order | O(log n + c) |
/// | `AddThreatScenario` | `RemoveThreatScenario` | O(k log n + k log k + p) |
/// | `RenameThreatScenario` | the same with the old name and provenance | O(log n) |
/// | `SetThreatCategory` | the same with the old category and provenance | O(log n + p) |
/// | `AddCatalogRef` | `RemoveCatalogRef` with the old provenance | O(log n + k) |
/// | `RemoveCatalogRef` | `AddCatalogRef` with the old provenance | O(log n + k) |
/// | `LinkDamage` | `UnlinkDamage` with the old provenance | O(log n + k) |
/// | `UnlinkDamage` | `LinkDamage` with the old provenance | O(log n + k) |
/// | `RemoveThreatScenario` | `AddThreatScenario`, fields in stored order | O(log n + k + p) |
/// | `AddAttackTree` | `RemoveAttackTree` | O(log n + V + E) |
/// | `SetStepFeasibility` | the same with the old level and provenance | O(log n + log V) |
/// | `SetStepFactor` | the same with the old level and provenance | O(log n + log V + F) |
/// | `SetThreatFeasibility` | the same with the old level and provenance | O(log n) |
/// | `SetThreatAttackTree` | the same with the old tree and provenance | O(log n) |
/// | `SetRiskTreatment` | the same with the old treatment and provenance | O(log n) |
/// | `RemoveAttackTree` | `AddAttackTree`, rated steps in node order | O(log n + V) |
/// | `AddSecurityControl` | `RemoveSecurityControl` | O(log n) |
/// | `RenameSecurityControl` | the same with the old name and provenance | O(log n) |
/// | `RemoveSecurityControl` | `AddSecurityControl` with the old fields and provenance | O(log n) |
/// | `SetStepMitigation` | the same with the old feasibility and provenance | O(log n + log V + m) |
/// | `AddCybersecurityGoal` | `RemoveCybersecurityGoal` | O(m log n + m log m) |
/// | `RenameCybersecurityGoal` | the same with the old name and provenance | O(log n) |
/// | `LinkGoalControl` | `UnlinkGoalControl` with the old provenance | O(log n + m) |
/// | `UnlinkGoalControl` | `LinkGoalControl` with the old provenance | O(log n + m) |
/// | `RemoveCybersecurityGoal` | `AddCybersecurityGoal`, controls in stored order | O(log n + m) |
/// | `AddCybersecurityClaim` | `RemoveCybersecurityClaim` | O(log n) |
/// | `RenameCybersecurityClaim` | the same with the old name and provenance | O(log n) |
/// | `RemoveCybersecurityClaim` | `AddCybersecurityClaim` with the old fields and provenance | O(log n) |
/// | `SetThreatGoal` | the same with the old goal and provenance | O(log n) |
/// | `SetThreatClaim` | the same with the old claim and provenance | O(log n) |
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let id = AssetId::new_v7();
/// let inverse = project.apply(Op::AddAsset {
///     id,
///     name: "Telematics unit".into(),
///     properties: m.all_properties(),
///     provenance: Provenance::human("alice", 1_700_000_000_000),
/// })?;
/// assert_eq!(inverse, Op::RemoveAsset { id });
/// project.apply(inverse)?;
/// assert_eq!(project, Project::new(m));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    /// Sets the description of an element, or the notes of the project
    /// ([`ElementRef::Project`]). Empty text clears it. Plain text, at most
    /// [`ElementRef::max_chars`] chars. An element that has a description
    /// cannot be removed until it is cleared.
    SetDescription {
        /// What the text belongs to; an element must exist.
        target: ElementRef,
        /// The new text.
        text: Arc<str>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Adds an asset. The property set must be non-empty.
    AddAsset {
        /// Unused asset ID.
        id: AssetId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Security properties of the asset.
        properties: PropertySet,
        /// Provenance of the asset creation.
        provenance: Provenance,
    },
    /// Renames an asset.
    RenameAsset {
        /// Existing asset.
        id: AssetId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Replaces an asset's properties. Must not drop a property that a
    /// threat targets.
    SetAssetProperties {
        /// Existing asset.
        id: AssetId,
        /// New non-empty property set.
        properties: PropertySet,
        /// Provenance of the property change.
        provenance: Provenance,
    },
    /// Removes an asset that no threat targets.
    RemoveAsset {
        /// Existing asset.
        id: AssetId,
    },
    /// Adds a damage scenario. Categories not listed are unrated.
    AddDamageScenario {
        /// Unused damage scenario ID.
        id: DamageId,
        /// Name.
        name: Arc<str>,
        /// Ratings, each category at most once.
        impacts: Vec<(ImpactCategory, Level)>,
        /// Provenance of the damage scenario creation.
        provenance: Provenance,
    },
    /// Renames a damage scenario.
    RenameDamageScenario {
        /// Existing damage scenario.
        id: DamageId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Sets or clears (`None` = unrated) one impact rating.
    SetImpact {
        /// Existing damage scenario.
        id: DamageId,
        /// The impact category.
        category: ImpactCategory,
        /// The new rating, or `None` for unrated.
        level: Option<Level>,
        /// Provenance of the impact rating change.
        provenance: Provenance,
    },
    /// Removes a damage scenario that no threat links to.
    RemoveDamageScenario {
        /// Existing damage scenario.
        id: DamageId,
    },
    /// Adds a threat scenario. The asset must carry every property the
    /// category violates.
    AddThreatScenario {
        /// Unused threat scenario ID.
        id: ThreatId,
        /// Name.
        name: Arc<str>,
        /// The targeted asset.
        asset: AssetId,
        /// The threat category.
        category: ThreatCategory,
        /// Catalog refs, unique, at most 16, in any order (stored sorted).
        catalog_refs: Vec<CatalogRef>,
        /// Linked damage scenarios, unique, at most 256, in any order
        /// (stored sorted by ID).
        damages: Vec<DamageId>,
        /// Provenance of the threat scenario creation.
        provenance: Provenance,
    },
    /// Renames a threat scenario.
    RenameThreatScenario {
        /// Existing threat scenario.
        id: ThreatId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Changes a threat's category. The asset must carry every property the
    /// new category violates.
    SetThreatCategory {
        /// Existing threat scenario.
        id: ThreatId,
        /// The new category.
        category: ThreatCategory,
        /// Provenance of the category change.
        provenance: Provenance,
    },
    /// Adds a catalog ref to a threat.
    AddCatalogRef {
        /// Existing threat scenario.
        threat: ThreatId,
        /// A ref not yet on the threat.
        catalog_ref: CatalogRef,
        /// Provenance of adding the catalog ref.
        provenance: Provenance,
    },
    /// Removes a catalog ref from a threat.
    RemoveCatalogRef {
        /// Existing threat scenario.
        threat: ThreatId,
        /// A ref on the threat.
        catalog_ref: CatalogRef,
        /// Provenance of removing the catalog ref.
        provenance: Provenance,
    },
    /// Links a threat to a damage scenario. A threat links at most 256.
    LinkDamage {
        /// Existing threat scenario.
        threat: ThreatId,
        /// Existing damage scenario, not yet linked.
        damage: DamageId,
        /// Provenance of linking the damage scenario.
        provenance: Provenance,
    },
    /// Unlinks a threat from a damage scenario.
    UnlinkDamage {
        /// Existing threat scenario.
        threat: ThreatId,
        /// A linked damage scenario.
        damage: DamageId,
        /// Provenance of unlinking the damage scenario.
        provenance: Provenance,
    },
    /// Removes a threat scenario.
    RemoveThreatScenario {
        /// Existing threat scenario.
        id: ThreatId,
    },
    /// Adds an attack tree with optional initial step feasibility ratings.
    /// Steps not listed are unrated (`None`).
    AddAttackTree {
        /// Unused attack tree ID.
        id: TreeId,
        /// The attack tree DAG.
        tree: AttackTree,
        /// Initial step feasibility ratings, each step at most once.
        step_feasibility: Vec<(NodeId, FeasibilityLevel)>,
        /// Provenance of the attack tree creation.
        provenance: Provenance,
    },
    /// Sets or clears (`None` = unrated) the feasibility rating of one
    /// step node in an attack tree.
    SetStepFeasibility {
        /// Existing attack tree.
        tree: TreeId,
        /// Existing step node in `tree`.
        node: NodeId,
        /// The new feasibility rating, or `None` for unrated.
        level: Option<FeasibilityLevel>,
        /// Provenance of the step feasibility change.
        provenance: Provenance,
    },
    /// Sets or clears (`None` = unrated) the rating of one attack potential
    /// factor on one step node in an attack tree.
    ///
    /// When every factor in the methodology's attack potential vocabulary is
    /// rated on `node`, [`Project::step_attack_potential`](crate::Project::step_attack_potential)
    /// resolves the step's score, band, and feasibility, and
    /// [`Project::effective_step_feasibility`](crate::Project::effective_step_feasibility)
    /// prefers that resolved feasibility over any direct
    /// [`Op::SetStepFeasibility`] rating.
    SetStepFactor {
        /// Existing attack tree.
        tree: TreeId,
        /// Existing step node in `tree`.
        node: NodeId,
        /// The attack potential factor.
        factor: FeasibilityFactor,
        /// The new factor rating, or `None` for unrated.
        level: Option<FactorLevel>,
        /// Provenance of the step factor change.
        provenance: Provenance,
    },
    /// Sets or clears (`None` = unrated) the direct feasibility rating of a
    /// threat scenario.
    ///
    /// Used during concept-phase TARA before an attack tree is constructed.
    /// When a threat scenario also links to an attack tree via
    /// [`Op::SetThreatAttackTree`], [`Project::evaluate_risk`](crate::Project::evaluate_risk)
    /// uses the attack tree's evaluated feasibility while preserving the
    /// direct rating on the threat scenario.
    SetThreatFeasibility {
        /// Existing threat scenario.
        threat: ThreatId,
        /// The new direct feasibility rating, or `None` for unrated.
        level: Option<FeasibilityLevel>,
        /// Provenance of the direct feasibility change.
        provenance: Provenance,
    },
    /// Links a threat scenario to an attack tree, replaces its linked
    /// attack tree, or unlinks it (`None`).
    SetThreatAttackTree {
        /// Existing threat scenario.
        threat: ThreatId,
        /// Existing attack tree, or `None` to unlink.
        tree: Option<TreeId>,
        /// Provenance of linking or unlinking the attack tree.
        provenance: Provenance,
    },
    /// Sets or clears (`None`) the risk treatment decision of a threat
    /// scenario.
    SetRiskTreatment {
        /// Existing threat scenario.
        threat: ThreatId,
        /// The new risk treatment decision, or `None` to clear.
        treatment: Option<RiskTreatment>,
        /// Provenance of the risk treatment change.
        provenance: Provenance,
    },
    /// Removes an attack tree that no threat scenario links to and that has
    /// no attached step mitigations or step factor ratings.
    RemoveAttackTree {
        /// Existing attack tree.
        id: TreeId,
    },
    /// Adds a security control.
    AddSecurityControl {
        /// Unused security control ID.
        id: ControlId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Provenance of the security control creation.
        provenance: Provenance,
    },
    /// Renames a security control.
    RenameSecurityControl {
        /// Existing security control.
        id: ControlId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Removes a security control that is not attached to any attack step.
    RemoveSecurityControl {
        /// Existing security control.
        id: ControlId,
    },
    /// Attaches, updates, or detaches (`None`) a [`SecurityControl`](crate::SecurityControl)
    /// mitigation on one step node in an attack tree.
    SetStepMitigation {
        /// Existing attack tree.
        tree: TreeId,
        /// Existing step node in `tree`.
        node: NodeId,
        /// Existing security control.
        control: ControlId,
        /// Residual feasibility cap imposed by `control` on `node`, or `None`
        /// to detach `control` from `node`.
        feasibility: Option<FeasibilityLevel>,
        /// Provenance of the mitigation change.
        provenance: Provenance,
    },
    /// Adds a cybersecurity goal.
    AddCybersecurityGoal {
        /// Unused cybersecurity goal ID.
        id: GoalId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Linked security controls, unique, at most 16, in any order
        /// (stored sorted by ID).
        controls: Vec<ControlId>,
        /// Provenance of the cybersecurity goal creation.
        provenance: Provenance,
    },
    /// Renames a cybersecurity goal.
    RenameCybersecurityGoal {
        /// Existing cybersecurity goal.
        id: GoalId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Links a cybersecurity goal to a security control. A goal links at most
    /// 16 controls.
    LinkGoalControl {
        /// Existing cybersecurity goal.
        goal: GoalId,
        /// Existing security control, not yet linked on `goal`.
        control: ControlId,
        /// Provenance of linking the security control.
        provenance: Provenance,
    },
    /// Unlinks a cybersecurity goal from a security control.
    UnlinkGoalControl {
        /// Existing cybersecurity goal.
        goal: GoalId,
        /// A security control linked on `goal`.
        control: ControlId,
        /// Provenance of unlinking the security control.
        provenance: Provenance,
    },
    /// Removes a cybersecurity goal that no threat scenario references.
    RemoveCybersecurityGoal {
        /// Existing cybersecurity goal.
        id: GoalId,
    },
    /// Adds a cybersecurity claim.
    AddCybersecurityClaim {
        /// Unused cybersecurity claim ID.
        id: ClaimId,
        /// Statement/name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Provenance of the cybersecurity claim creation.
        provenance: Provenance,
    },
    /// Renames a cybersecurity claim.
    RenameCybersecurityClaim {
        /// Existing cybersecurity claim.
        id: ClaimId,
        /// New statement/name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Removes a cybersecurity claim that no threat scenario references.
    RemoveCybersecurityClaim {
        /// Existing cybersecurity claim.
        id: ClaimId,
    },
    /// Links a threat scenario to a cybersecurity goal, replaces its linked
    /// goal, or unlinks it (`None`).
    SetThreatGoal {
        /// Existing threat scenario.
        threat: ThreatId,
        /// Existing cybersecurity goal, or `None` to unlink.
        goal: Option<GoalId>,
        /// Provenance of linking or unlinking the cybersecurity goal.
        provenance: Provenance,
    },
    /// Links a threat scenario to a cybersecurity claim, replaces its linked
    /// claim, or unlinks it (`None`).
    SetThreatClaim {
        /// Existing threat scenario.
        threat: ThreatId,
        /// Existing cybersecurity claim, or `None` to unlink.
        claim: Option<ClaimId>,
        /// Provenance of linking or unlinking the cybersecurity claim.
        provenance: Provenance,
    },
}

impl Op {
    /// Returns the provenance carried by this operation, or `None` for the
    /// seven entity-removal variants (`RemoveAsset`, `RemoveDamageScenario`,
    /// `RemoveThreatScenario`, `RemoveAttackTree`, `RemoveSecurityControl`,
    /// `RemoveCybersecurityGoal`, `RemoveCybersecurityClaim`).
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, Op, Provenance};
    ///
    /// let id = AssetId::new_v7();
    /// let prov = Provenance::human("alice", 100);
    /// let rename = Op::RenameAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     provenance: prov.clone(),
    /// };
    /// assert_eq!(rename.provenance(), Some(&prov));
    /// assert_eq!(Op::RemoveAsset { id }.provenance(), None);
    /// ```
    #[must_use]
    pub const fn provenance(&self) -> Option<&Provenance> {
        match self {
            Self::SetDescription { provenance, .. }
            | Self::AddAsset { provenance, .. }
            | Self::RenameAsset { provenance, .. }
            | Self::SetAssetProperties { provenance, .. }
            | Self::AddDamageScenario { provenance, .. }
            | Self::RenameDamageScenario { provenance, .. }
            | Self::SetImpact { provenance, .. }
            | Self::AddThreatScenario { provenance, .. }
            | Self::RenameThreatScenario { provenance, .. }
            | Self::SetThreatCategory { provenance, .. }
            | Self::AddCatalogRef { provenance, .. }
            | Self::RemoveCatalogRef { provenance, .. }
            | Self::LinkDamage { provenance, .. }
            | Self::UnlinkDamage { provenance, .. }
            | Self::AddAttackTree { provenance, .. }
            | Self::SetStepFeasibility { provenance, .. }
            | Self::SetStepFactor { provenance, .. }
            | Self::SetThreatFeasibility { provenance, .. }
            | Self::SetThreatAttackTree { provenance, .. }
            | Self::SetRiskTreatment { provenance, .. }
            | Self::AddSecurityControl { provenance, .. }
            | Self::RenameSecurityControl { provenance, .. }
            | Self::SetStepMitigation { provenance, .. }
            | Self::AddCybersecurityGoal { provenance, .. }
            | Self::RenameCybersecurityGoal { provenance, .. }
            | Self::LinkGoalControl { provenance, .. }
            | Self::UnlinkGoalControl { provenance, .. }
            | Self::AddCybersecurityClaim { provenance, .. }
            | Self::RenameCybersecurityClaim { provenance, .. }
            | Self::SetThreatGoal { provenance, .. }
            | Self::SetThreatClaim { provenance, .. } => Some(provenance),
            Self::RemoveAsset { .. }
            | Self::RemoveDamageScenario { .. }
            | Self::RemoveThreatScenario { .. }
            | Self::RemoveAttackTree { .. }
            | Self::RemoveSecurityControl { .. }
            | Self::RemoveCybersecurityGoal { .. }
            | Self::RemoveCybersecurityClaim { .. } => None,
        }
    }
}
