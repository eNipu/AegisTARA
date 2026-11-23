//! Typed operations on a [`Project`](crate::Project).

use std::sync::Arc;

use crate::asset::PropertySet;
use crate::control::RiskTreatment;
use crate::description::ElementRef;
use crate::element::{AttrValue, ElementKind, LinkTarget};
use crate::id::{
    AssetId, ClaimId, ComponentId, ConditionId, ControlId, DamageId, DataFlowId, ElementId, GoalId,
    InterfaceId, NodeId, PartyId, ThreatId, TreeId, TrustBoundaryId,
};
use crate::item::ItemRef;
use crate::methodology::{
    AttackType, CatalogRef, ControlKind, FactorLevel, FeasibilityFactor, FeasibilityLevel,
    ImpactCategory, InterfaceKind, Level, ThreatCategory,
};
use crate::party::PartyKind;
use crate::provenance::Provenance;
use crate::tree::{AttackTree, GateKind, Node};

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
/// | `SetReviewMark` | the same with the old mark and tree provenance | O(log n) |
/// | `SetProjectLanguages` | the same with the old list and provenance | O(l) |
/// | `AddTreeNode` | `RemoveTreeNode` with the old tree provenance | O(V + E) |
/// | `RemoveTreeNode` | `AddTreeNode` with the node, its place and the old tree provenance | O(V + E) |
/// | `LinkTreeNode` | `UnlinkTreeNode` with the old tree provenance | O(k + d + e) |
/// | `UnlinkTreeNode` | `LinkTreeNode` at the old place with the old tree provenance | O(k + p) |
/// | `RenameTreeNode` | the same with the old label and provenance | O(1) |
/// | `SetTreeNodeGate` | the same with the old gate and provenance | O(1) |
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
/// | `SetStepVia` | the same with the old item and provenance | O(log n + log V) |
/// | `SetStepAttackType` | the same with the old attack type and provenance | O(log n + log V) |
/// | `SetThreatFeasibility` | the same with the old level and provenance | O(log n) |
/// | `SetThreatAttackTree` | the same with the old tree and provenance | O(log n) |
/// | `SetRiskTreatment` | the same with the old treatment and provenance | O(log n) |
/// | `RemoveAttackTree` | `AddAttackTree`, rated steps in node order | O(log n + V) |
/// | `AddSecurityControl` | `RemoveSecurityControl` | O(log n) |
/// | `RenameSecurityControl` | the same with the old name and provenance | O(log n) |
/// | `RemoveSecurityControl` | `AddSecurityControl` with the old fields and provenance | O(log n) |
/// | `ApplyControl` | `UnapplyControl` with the old control provenance | O(log n + t) |
/// | `UnapplyControl` | `ApplyControl` with the old control provenance | O(log n + t) |
/// | `SetControlRule` | the same with the old ceiling and control provenance | O(log n + r) |
/// | `SetControlKind` | the same with the old kind and control provenance | O(log n) |
/// | `SetControlStatus` | the same with the old status and control provenance | O(log n) |
/// | `SetStepMitigation` | the same with the old feasibility and provenance | O(log n + log V + m) |
/// | `AddCybersecurityGoal` | `RemoveCybersecurityGoal` | O(m log n + m log m) |
/// | `RenameCybersecurityGoal` | the same with the old name and provenance | O(log n) |
/// | `LinkGoalControl` | `UnlinkGoalControl` with the old provenance | O(log n + m) |
/// | `UnlinkGoalControl` | `LinkGoalControl` with the old provenance | O(log n + m) |
/// | `LinkGoalParty` | `UnlinkGoalParty` with the old provenance | O(log n + m) |
/// | `UnlinkGoalParty` | `LinkGoalParty` with the old provenance | O(log n + m) |
/// | `RemoveCybersecurityGoal` | `AddCybersecurityGoal`, controls in stored order | O(log n + m) |
/// | `AddCybersecurityClaim` | `RemoveCybersecurityClaim` | O(log n) |
/// | `RenameCybersecurityClaim` | the same with the old name and provenance | O(log n) |
/// | `LinkClaimParty` | `UnlinkClaimParty` with the old provenance | O(log n + m) |
/// | `UnlinkClaimParty` | `LinkClaimParty` with the old provenance | O(log n + m) |
/// | `RemoveCybersecurityClaim` | `AddCybersecurityClaim` with the old fields and provenance | O(log n) |
/// | `AddParty` | `RemoveParty` | O(log n) |
/// | `RenameParty` | the same with the old name and provenance | O(log n) |
/// | `SetPartyKind` | the same with the old kind and provenance | O(log n) |
/// | `RemoveParty` | `AddParty` with the old fields and provenance | O(log n + goals + claims) |
/// | `AddCondition` | `RemoveCondition` | O(log n) |
/// | `RenameCondition` | the same with the old name and provenance | O(log n) |
/// | `SetConditionPresent` | the same with the old presence and provenance | O(log n) |
/// | `RemoveCondition` | `AddCondition` with the old fields and provenance | O(log n) |
/// | `NeedCondition` | `UnneedCondition` with the same provenance | O(log n) |
/// | `UnneedCondition` | `NeedCondition` with the same provenance | O(log n) |
/// | `NeedStepCondition` | `UnneedStepCondition` with the same provenance | O(log n) |
/// | `UnneedStepCondition` | `NeedStepCondition` with the same provenance | O(log n) |
/// | `SetThreatGoal` | the same with the old goal and provenance | O(log n) |
/// | `SetThreatClaim` | the same with the old claim and provenance | O(log n) |
/// | `SetThreatScaling` | the same with the old rating, reason and provenance | O(log n + r) |
/// | `SetImpactRationale` | the same with the old text and provenance | O(log n + r) |
/// | `SetFeasibilityRationale` | the same with the old text and provenance | O(log n + r) |
/// | `SetDamageLinkRationale` | the same with the old text and provenance | O(log n + r) |
/// | `AddElement` | `RemoveElement` | O(log n) |
/// | `RenameElement` | the same with the old name and provenance | O(log n) |
/// | `RemoveElement` | `AddElement` with the old kind, name and provenance | O(log n) |
/// | `SetElementAttribute` | the same with the old value (`None` if unset) and provenance | O(log n + a) |
/// | `LinkElement` | `UnlinkElement` with the element's old provenance | O(log n + l) |
/// | `UnlinkElement` | `LinkElement` with the element's old provenance | O(log n + l) |
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
    /// Marks or unmarks a step as "its rating needs review". A library
    /// insert marks the ratings it brings; a person reviews and unmarks. A
    /// marked step cannot be removed, and the linter reports it.
    SetReviewMark {
        /// Existing attack tree.
        tree: TreeId,
        /// A step of the tree.
        node: NodeId,
        /// Marked (needs review) or not.
        marked: bool,
        /// Provenance of the change.
        provenance: Provenance,
    },
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
    /// Sets or clears one translation of a description (FND-04, L10N-08).
    /// Empty `text` clears the translation for `lang`. Plain text, at most
    /// [`ElementRef::max_chars`] chars, and `lang` is a BCP 47 tag.
    SetDescriptionTranslation {
        /// What the description belongs to; an element must exist and have a
        /// description.
        target: ElementRef,
        /// The language tag of the translation.
        lang: Arc<str>,
        /// The translated text; empty clears it.
        text: Arc<str>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Sets the languages the project keeps description translations in, in
    /// display order; empty means the project is single-language. At most
    /// [`MAX_DESCRIPTION_TRANSLATIONS`](crate::MAX_DESCRIPTION_TRANSLATIONS)
    /// BCP 47 tags, none repeated. Stored translations in other languages
    /// stay.
    SetProjectLanguages {
        /// The language tags, for example `["ja"]`.
        langs: Arc<[Arc<str>]>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Adds one node to an attack tree, as child number `position` of the
    /// gate `parent`, at arena index `index`. A node added at the end has
    /// `index` = the number of nodes and `position` = the number of children
    /// of `parent`. Trees of up to [`MAX_ATTACK_TREE_NODES`](crate::MAX_ATTACK_TREE_NODES)
    /// nodes are built from these ops.
    AddTreeNode {
        /// Existing attack tree.
        tree: TreeId,
        /// Existing gate of the tree.
        parent: NodeId,
        /// Unused node ID.
        id: NodeId,
        /// A step, or a gate without children.
        node: Node,
        /// Arena index of the new node: after `parent`, at most the number
        /// of nodes.
        index: u32,
        /// Place among the children of `parent`.
        position: u32,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Removes a node nothing hangs on: not the root, no children, not
    /// shared, and for a step no rating, factors or controls. Clear those
    /// first (an edit does it in the same batch).
    RemoveTreeNode {
        /// Existing attack tree.
        tree: TreeId,
        /// The node.
        id: NodeId,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Shares the node `child` with a second gate: adds it to the children
    /// of `parent` at `position`, as extra parent number `slot` of `child`.
    /// Rejected if it would close a cycle.
    LinkTreeNode {
        /// Existing attack tree.
        tree: TreeId,
        /// The gate that gets the child.
        parent: NodeId,
        /// The shared node.
        child: NodeId,
        /// Place among the children of `parent`.
        position: u32,
        /// Place among the extra parents of `child` (their number, for a
        /// new link).
        slot: u32,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Removes an extra edge made by `LinkTreeNode`.
    UnlinkTreeNode {
        /// Existing attack tree.
        tree: TreeId,
        /// The gate.
        parent: NodeId,
        /// The shared node.
        child: NodeId,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Changes the label of a node.
    RenameTreeNode {
        /// Existing attack tree.
        tree: TreeId,
        /// The node.
        id: NodeId,
        /// New label, non-empty after trimming, at most 256 chars.
        label: Arc<str>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Changes how a gate combines its children.
    SetTreeNodeGate {
        /// Existing attack tree.
        tree: TreeId,
        /// A gate of the tree.
        id: NodeId,
        /// The new gate kind.
        gate: GateKind,
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
    /// Sets or clears (`None`) the component or interface one attack step
    /// goes through (CFG-01). Steps only; a gate is rejected with
    /// [`ModelError::NotAStep`](crate::ModelError::NotAStep).
    SetStepVia {
        /// Existing attack tree.
        tree: TreeId,
        /// Existing step node in `tree`.
        node: NodeId,
        /// The live component or interface, or `None` for nothing.
        via: Option<ItemRef>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Sets or clears (`None`) the pack-defined attack type of one attack
    /// step (CFG-02). Steps only.
    SetStepAttackType {
        /// Existing attack tree.
        tree: TreeId,
        /// Existing step node in `tree`.
        node: NodeId,
        /// The attack type in the project's methodology, or `None`.
        attack_type: Option<AttackType>,
        /// Provenance of the change.
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
    /// Removes a security control that is not attached to any attack step,
    /// linked by any goal, applied to any component or interface, or carrying
    /// any control rules or pack preset.
    RemoveSecurityControl {
        /// Existing security control.
        id: ControlId,
    },
    /// Applies a [`SecurityControl`](crate::SecurityControl) as a setting to a
    /// component or interface (CFG-03, D26). A control may be applied to at
    /// most [`MAX_CONTROL_TARGETS`](crate::MAX_CONTROL_TARGETS) targets.
    ApplyControl {
        /// Existing security control.
        control: ControlId,
        /// Existing component or interface not yet targeted by `control`.
        target: ItemRef,
        /// Provenance of applying the control.
        provenance: Provenance,
    },
    /// Removes a [`SecurityControl`](crate::SecurityControl) from a component
    /// or interface it is currently applied to (CFG-03, D26).
    UnapplyControl {
        /// Existing security control.
        control: ControlId,
        /// A component or interface currently targeted by `control`.
        target: ItemRef,
        /// Provenance of unapplying the control.
        provenance: Provenance,
    },
    /// Sets, updates, or clears (`None`) the residual feasibility ceiling a
    /// [`SecurityControl`](crate::SecurityControl) imposes on attack steps of
    /// `attack_type` going through any of its targets (CFG-03, D26). A control
    /// may carry at most [`MAX_CONTROL_RULES`](crate::MAX_CONTROL_RULES) rules.
    SetControlRule {
        /// Existing security control.
        control: ControlId,
        /// Valid attack type in the project's methodology.
        attack_type: AttackType,
        /// Residual feasibility ceiling for `attack_type`, or `None` to remove
        /// the rule.
        ceiling: Option<FeasibilityLevel>,
        /// Provenance of the rule change.
        provenance: Provenance,
    },
    /// Sets or clears (`None`) the pack preset a
    /// [`SecurityControl`](crate::SecurityControl) was created from (CFG-06,
    /// D26). The kind is a label: it changes no rule and no risk number.
    SetControlKind {
        /// Existing security control.
        control: ControlId,
        /// A control kind of the project's methodology, or `None`.
        kind: Option<ControlKind>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Sets the lifecycle status ([`ControlStatus`](crate::ControlStatus)) of a
    /// [`SecurityControl`](crate::SecurityControl) (CFG-10, D28).
    SetControlStatus {
        /// Existing security control.
        control: ControlId,
        /// New lifecycle status (`Implemented`, `Planned`, or `Off`).
        status: crate::control::ControlStatus,
        /// Provenance of the change.
        provenance: Provenance,
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
    /// Lists a party as responsible for a goal (at most 16 parties).
    LinkGoalParty {
        /// Existing cybersecurity goal.
        goal: GoalId,
        /// Existing party, not yet listed on `goal`.
        party: PartyId,
        /// Provenance of listing the party as responsible.
        provenance: Provenance,
    },
    /// Removes a party from the responsible list of a cybersecurity goal.
    UnlinkGoalParty {
        /// Existing cybersecurity goal.
        goal: GoalId,
        /// A party listed on `goal`.
        party: PartyId,
        /// Provenance of removing the party from the responsible list.
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
    /// Lists a party as responsible for a claim (at most 16 parties).
    LinkClaimParty {
        /// Existing cybersecurity claim.
        claim: ClaimId,
        /// Existing party, not yet listed on `claim`.
        party: PartyId,
        /// Provenance of listing the party as responsible.
        provenance: Provenance,
    },
    /// Removes a party from the responsible list of a cybersecurity claim.
    UnlinkClaimParty {
        /// Existing cybersecurity claim.
        claim: ClaimId,
        /// A party listed on `claim`.
        party: PartyId,
        /// Provenance of removing the party from the responsible list.
        provenance: Provenance,
    },
    /// Removes a cybersecurity claim that no threat scenario references.
    RemoveCybersecurityClaim {
        /// Existing cybersecurity claim.
        id: ClaimId,
    },
    /// Adds a party to the project's party list.
    AddParty {
        /// Unused party ID.
        id: PartyId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// The party's role.
        kind: PartyKind,
        /// Provenance of the party creation.
        provenance: Provenance,
    },
    /// Renames a party.
    RenameParty {
        /// Existing party.
        id: PartyId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Changes a party's role.
    SetPartyKind {
        /// Existing party.
        id: PartyId,
        /// New role.
        kind: PartyKind,
        /// Provenance of the kind change.
        provenance: Provenance,
    },
    /// Removes a party. The inverse restores its fields and provenance.
    RemoveParty {
        /// Existing party.
        id: PartyId,
    },
    /// Adds a condition to the project's condition list.
    AddCondition {
        /// Unused condition ID.
        id: ConditionId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Plain-text description, at most `MAX_DESCRIPTION_CHARS` chars.
        description: Arc<str>,
        /// Whether the condition is present in the project.
        present: bool,
        /// Provenance of the condition creation.
        provenance: Provenance,
    },
    /// Renames a condition.
    RenameCondition {
        /// Existing condition.
        id: ConditionId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Sets whether a condition is present.
    SetConditionPresent {
        /// Existing condition.
        id: ConditionId,
        /// New presence.
        present: bool,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Removes a condition. The inverse restores all fields and provenance.
    RemoveCondition {
        /// Existing condition.
        id: ConditionId,
    },
    /// Records that an element needs a condition, capped at
    /// `MAX_CONDITIONS_PER_ELEMENT` and stored sorted by public ID.
    NeedCondition {
        /// The element that needs the condition.
        element: ElementRef,
        /// Existing condition.
        condition: ConditionId,
        /// Provenance of linking the condition.
        provenance: Provenance,
    },
    /// Stops an element from needing a condition.
    UnneedCondition {
        /// The element that no longer needs the condition.
        element: ElementRef,
        /// A condition the element needs.
        condition: ConditionId,
        /// Provenance of unlinking the condition.
        provenance: Provenance,
    },
    /// Records that one attack-tree step needs a condition.
    NeedStepCondition {
        /// Existing attack tree.
        tree: TreeId,
        /// An existing step node in `tree`.
        node: NodeId,
        /// Existing condition.
        condition: ConditionId,
        /// Provenance of linking the condition.
        provenance: Provenance,
    },
    /// Stops one attack-tree step from needing a condition.
    UnneedStepCondition {
        /// Existing attack tree.
        tree: TreeId,
        /// An existing step node in `tree`.
        node: NodeId,
        /// A condition the step needs.
        condition: ConditionId,
        /// Provenance of unlinking the condition.
        provenance: Provenance,
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
    /// Sets why a damage scenario's impact is rated as it is (FND-09):
    /// plain text, at most [`MAX_DESCRIPTION_CHARS`](crate::MAX_DESCRIPTION_CHARS)
    /// chars; empty clears it.
    SetImpactRationale {
        /// Existing damage scenario.
        damage: DamageId,
        /// The rationale.
        text: Arc<str>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Sets why a threat scenario's feasibility is rated as it is (FND-09):
    /// plain text, at most [`MAX_DESCRIPTION_CHARS`](crate::MAX_DESCRIPTION_CHARS)
    /// chars; empty clears it.
    SetFeasibilityRationale {
        /// Existing threat scenario.
        threat: ThreatId,
        /// The rationale.
        text: Arc<str>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Sets why a threat is linked to a damage scenario (FND-09): plain text,
    /// at most [`MAX_DESCRIPTION_CHARS`](crate::MAX_DESCRIPTION_CHARS)
    /// chars; empty clears it.
    SetDamageLinkRationale {
        /// Existing threat scenario.
        threat: ThreatId,
        /// An existing damage scenario already linked to the threat.
        damage: DamageId,
        /// The rationale.
        text: Arc<str>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Rates whether a threat scenario scales to many vehicles (FND-10):
    /// `Some(true)`, `Some(false)`, or `None` for not rated, with a reasoning
    /// text (plain text, at most [`MAX_DESCRIPTION_CHARS`](crate::MAX_DESCRIPTION_CHARS)
    /// chars, may be empty).
    SetThreatScaling {
        /// Existing threat scenario.
        threat: ThreatId,
        /// Whether it scales, or `None` for not rated.
        scales: Option<bool>,
        /// Why it scales or not.
        reason: Arc<str>,
        /// Provenance of the rating.
        provenance: Provenance,
    },
    /// Adds a component to the item definition.
    AddComponent {
        /// Unused component ID.
        id: ComponentId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Optional parent component for hierarchical decomposition.
        parent: Option<ComponentId>,
        /// Provenance of the component creation.
        provenance: Provenance,
    },
    /// Renames a component.
    RenameComponent {
        /// Existing component.
        id: ComponentId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Sets, replaces, or clears (`None`) a component's parent.
    SetComponentParent {
        /// Existing component.
        id: ComponentId,
        /// Existing parent component, or `None` for top-level.
        parent: Option<ComponentId>,
        /// Provenance of the parent change.
        provenance: Provenance,
    },
    /// Puts a top-level component in a system, moves it to another, or
    /// takes it out (`None`).
    SetComponentSystem {
        /// Existing component with no parent.
        id: ComponentId,
        /// Existing element of kind `System`, or `None`.
        system: Option<ElementId>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Removes a component that has no child components, no interfaces, no
    /// data flows, no trust boundary membership, and no bound assets.
    RemoveComponent {
        /// Existing component.
        id: ComponentId,
    },
    /// Adds an interface on a component.
    AddInterface {
        /// Unused interface ID.
        id: InterfaceId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Existing component that exposes this interface.
        component: ComponentId,
        /// Methodology-defined interface kind.
        kind: InterfaceKind,
        /// Provenance of the interface creation.
        provenance: Provenance,
    },
    /// Renames an interface.
    RenameInterface {
        /// Existing interface.
        id: InterfaceId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Changes an interface's methodology-defined kind.
    SetInterfaceKind {
        /// Existing interface.
        id: InterfaceId,
        /// New interface kind.
        kind: InterfaceKind,
        /// Provenance of the kind change.
        provenance: Provenance,
    },
    /// Removes an interface that no data flow references.
    RemoveInterface {
        /// Existing interface.
        id: InterfaceId,
    },
    /// Adds a directed data flow between two components.
    AddDataFlow {
        /// Unused data flow ID.
        id: DataFlowId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Source component.
        from: ComponentId,
        /// Destination component.
        to: ComponentId,
        /// Optional interface on `from` or `to` over which the flow travels.
        interface: Option<InterfaceId>,
        /// Provenance of the data flow creation.
        provenance: Provenance,
    },
    /// Renames a data flow.
    RenameDataFlow {
        /// Existing data flow.
        id: DataFlowId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Sets, replaces, or clears (`None`) the interface of a data flow.
    SetDataFlowInterface {
        /// Existing data flow.
        id: DataFlowId,
        /// Existing interface on `from` or `to`, or `None` to clear.
        interface: Option<InterfaceId>,
        /// Provenance of the interface change.
        provenance: Provenance,
    },
    /// Removes a data flow (must have no linked interface).
    RemoveDataFlow {
        /// Existing data flow.
        id: DataFlowId,
    },
    /// Adds a trust boundary grouping a set of components.
    AddTrustBoundary {
        /// Unused trust boundary ID.
        id: TrustBoundaryId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Member components, unique, at most 64 (stored sorted by ID).
        components: Vec<ComponentId>,
        /// Provenance of the trust boundary creation.
        provenance: Provenance,
    },
    /// Renames a trust boundary.
    RenameTrustBoundary {
        /// Existing trust boundary.
        id: TrustBoundaryId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Adds a component to a trust boundary.
    AddBoundaryComponent {
        /// Existing trust boundary.
        boundary: TrustBoundaryId,
        /// Existing component not yet in `boundary`.
        component: ComponentId,
        /// Provenance of adding the member component.
        provenance: Provenance,
    },
    /// Removes a component from a trust boundary.
    RemoveBoundaryComponent {
        /// Existing trust boundary.
        boundary: TrustBoundaryId,
        /// A component currently in `boundary`.
        component: ComponentId,
        /// Provenance of removing the member component.
        provenance: Provenance,
    },
    /// Removes a trust boundary.
    RemoveTrustBoundary {
        /// Existing trust boundary.
        id: TrustBoundaryId,
    },
    /// Binds an asset to a component, replaces its bound component, or
    /// unbinds it (`None`).
    SetAssetComponent {
        /// Existing asset.
        asset: AssetId,
        /// Existing component, or `None` to unbind.
        component: Option<ComponentId>,
        /// Provenance of binding or unbinding the component.
        provenance: Provenance,
    },
    /// Adds an element of a generic kind, with no attributes and no links.
    AddElement {
        /// Unused element ID.
        id: ElementId,
        /// Kind of the element.
        kind: ElementKind,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Provenance of the element creation.
        provenance: Provenance,
    },
    /// Renames an element.
    RenameElement {
        /// Existing element.
        id: ElementId,
        /// New name.
        name: Arc<str>,
        /// Provenance of the rename.
        provenance: Provenance,
    },
    /// Removes an element. Refused while it has any attribute or link.
    RemoveElement {
        /// Existing element.
        id: ElementId,
    },
    /// Sets (`Some`) or clears (`None`) one extra attribute of an element (D34).
    SetElementAttribute {
        /// Existing element.
        id: ElementId,
        /// Attribute key, pack key syntax.
        key: Arc<str>,
        /// New value, or `None` to clear.
        value: Option<AttrValue>,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Links an element to a target its kind allows (a threat scenario for a misuse case).
    LinkElement {
        /// Existing element.
        id: ElementId,
        /// What to link.
        target: LinkTarget,
        /// Provenance of the change.
        provenance: Provenance,
    },
    /// Removes one link of an element.
    UnlinkElement {
        /// Existing element.
        id: ElementId,
        /// The linked target.
        target: LinkTarget,
        /// Provenance of the change.
        provenance: Provenance,
    },
}

impl Op {
    /// Returns the provenance carried by this operation, or `None` for the
    /// entity-removal variants (`RemoveAsset`, `RemoveDamageScenario`,
    /// `RemoveThreatScenario`, `RemoveAttackTree`, `RemoveSecurityControl`,
    /// `RemoveCybersecurityGoal`, `RemoveCybersecurityClaim`, `RemoveParty`,
    /// `RemoveCondition`).
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
    #[allow(clippy::too_many_lines)]
    pub const fn provenance(&self) -> Option<&Provenance> {
        match self {
            Self::SetDescription { provenance, .. }
            | Self::SetDescriptionTranslation { provenance, .. }
            | Self::SetReviewMark { provenance, .. }
            | Self::SetProjectLanguages { provenance, .. }
            | Self::AddTreeNode { provenance, .. }
            | Self::RemoveTreeNode { provenance, .. }
            | Self::LinkTreeNode { provenance, .. }
            | Self::UnlinkTreeNode { provenance, .. }
            | Self::RenameTreeNode { provenance, .. }
            | Self::SetTreeNodeGate { provenance, .. }
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
            | Self::SetStepVia { provenance, .. }
            | Self::SetStepAttackType { provenance, .. }
            | Self::SetThreatFeasibility { provenance, .. }
            | Self::SetThreatAttackTree { provenance, .. }
            | Self::SetRiskTreatment { provenance, .. }
            | Self::AddSecurityControl { provenance, .. }
            | Self::RenameSecurityControl { provenance, .. }
            | Self::ApplyControl { provenance, .. }
            | Self::UnapplyControl { provenance, .. }
            | Self::SetControlRule { provenance, .. }
            | Self::SetControlKind { provenance, .. }
            | Self::SetControlStatus { provenance, .. }
            | Self::SetStepMitigation { provenance, .. }
            | Self::AddCybersecurityGoal { provenance, .. }
            | Self::RenameCybersecurityGoal { provenance, .. }
            | Self::LinkGoalControl { provenance, .. }
            | Self::UnlinkGoalControl { provenance, .. }
            | Self::LinkGoalParty { provenance, .. }
            | Self::UnlinkGoalParty { provenance, .. }
            | Self::AddCybersecurityClaim { provenance, .. }
            | Self::RenameCybersecurityClaim { provenance, .. }
            | Self::LinkClaimParty { provenance, .. }
            | Self::UnlinkClaimParty { provenance, .. }
            | Self::AddParty { provenance, .. }
            | Self::RenameParty { provenance, .. }
            | Self::SetPartyKind { provenance, .. }
            | Self::AddCondition { provenance, .. }
            | Self::RenameCondition { provenance, .. }
            | Self::SetConditionPresent { provenance, .. }
            | Self::NeedCondition { provenance, .. }
            | Self::UnneedCondition { provenance, .. }
            | Self::NeedStepCondition { provenance, .. }
            | Self::UnneedStepCondition { provenance, .. }
            | Self::AddElement { provenance, .. }
            | Self::RenameElement { provenance, .. }
            | Self::SetElementAttribute { provenance, .. }
            | Self::LinkElement { provenance, .. }
            | Self::UnlinkElement { provenance, .. }
            | Self::SetThreatGoal { provenance, .. }
            | Self::SetThreatClaim { provenance, .. }
            | Self::SetThreatScaling { provenance, .. }
            | Self::SetImpactRationale { provenance, .. }
            | Self::SetFeasibilityRationale { provenance, .. }
            | Self::SetDamageLinkRationale { provenance, .. }
            | Self::AddComponent { provenance, .. }
            | Self::RenameComponent { provenance, .. }
            | Self::SetComponentParent { provenance, .. }
            | Self::SetComponentSystem { provenance, .. }
            | Self::AddInterface { provenance, .. }
            | Self::RenameInterface { provenance, .. }
            | Self::SetInterfaceKind { provenance, .. }
            | Self::AddDataFlow { provenance, .. }
            | Self::RenameDataFlow { provenance, .. }
            | Self::SetDataFlowInterface { provenance, .. }
            | Self::AddTrustBoundary { provenance, .. }
            | Self::RenameTrustBoundary { provenance, .. }
            | Self::AddBoundaryComponent { provenance, .. }
            | Self::RemoveBoundaryComponent { provenance, .. }
            | Self::SetAssetComponent { provenance, .. } => Some(provenance),
            Self::RemoveAsset { .. }
            | Self::RemoveDamageScenario { .. }
            | Self::RemoveThreatScenario { .. }
            | Self::RemoveAttackTree { .. }
            | Self::RemoveSecurityControl { .. }
            | Self::RemoveCybersecurityGoal { .. }
            | Self::RemoveCybersecurityClaim { .. }
            | Self::RemoveParty { .. }
            | Self::RemoveCondition { .. }
            | Self::RemoveElement { .. }
            | Self::RemoveComponent { .. }
            | Self::RemoveInterface { .. }
            | Self::RemoveDataFlow { .. }
            | Self::RemoveTrustBoundary { .. } => None,
        }
    }

    /// Returns a copy of this operation with its [`Provenance`] replaced by
    /// `new_provenance`. Entity-removal variants that carry no provenance are
    /// returned unchanged. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, Op, Provenance, ProvenanceSource};
    ///
    /// let id = AssetId::new_v7();
    /// let op = Op::RenameAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     provenance: Provenance::default(),
    /// }
    /// .with_provenance(Provenance::ai("alice", 1_700_000_000_000));
    /// assert_eq!(
    ///     op.provenance().map(Provenance::source),
    ///     Some(ProvenanceSource::Ai)
    /// );
    /// ```
    #[allow(clippy::too_many_lines)] // one arm per op kind
    #[must_use]
    pub fn with_provenance(mut self, new_provenance: Provenance) -> Self {
        match &mut self {
            Self::SetDescription { provenance, .. }
            | Self::SetDescriptionTranslation { provenance, .. }
            | Self::SetReviewMark { provenance, .. }
            | Self::SetProjectLanguages { provenance, .. }
            | Self::AddTreeNode { provenance, .. }
            | Self::RemoveTreeNode { provenance, .. }
            | Self::LinkTreeNode { provenance, .. }
            | Self::UnlinkTreeNode { provenance, .. }
            | Self::RenameTreeNode { provenance, .. }
            | Self::SetTreeNodeGate { provenance, .. }
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
            | Self::SetStepVia { provenance, .. }
            | Self::SetStepAttackType { provenance, .. }
            | Self::SetThreatFeasibility { provenance, .. }
            | Self::SetThreatAttackTree { provenance, .. }
            | Self::SetRiskTreatment { provenance, .. }
            | Self::AddSecurityControl { provenance, .. }
            | Self::RenameSecurityControl { provenance, .. }
            | Self::ApplyControl { provenance, .. }
            | Self::UnapplyControl { provenance, .. }
            | Self::SetControlRule { provenance, .. }
            | Self::SetControlKind { provenance, .. }
            | Self::SetControlStatus { provenance, .. }
            | Self::SetStepMitigation { provenance, .. }
            | Self::AddCybersecurityGoal { provenance, .. }
            | Self::RenameCybersecurityGoal { provenance, .. }
            | Self::LinkGoalControl { provenance, .. }
            | Self::UnlinkGoalControl { provenance, .. }
            | Self::LinkGoalParty { provenance, .. }
            | Self::UnlinkGoalParty { provenance, .. }
            | Self::AddCybersecurityClaim { provenance, .. }
            | Self::RenameCybersecurityClaim { provenance, .. }
            | Self::LinkClaimParty { provenance, .. }
            | Self::UnlinkClaimParty { provenance, .. }
            | Self::AddParty { provenance, .. }
            | Self::RenameParty { provenance, .. }
            | Self::SetPartyKind { provenance, .. }
            | Self::AddCondition { provenance, .. }
            | Self::RenameCondition { provenance, .. }
            | Self::SetConditionPresent { provenance, .. }
            | Self::NeedCondition { provenance, .. }
            | Self::UnneedCondition { provenance, .. }
            | Self::NeedStepCondition { provenance, .. }
            | Self::UnneedStepCondition { provenance, .. }
            | Self::AddElement { provenance, .. }
            | Self::RenameElement { provenance, .. }
            | Self::SetElementAttribute { provenance, .. }
            | Self::LinkElement { provenance, .. }
            | Self::UnlinkElement { provenance, .. }
            | Self::SetThreatGoal { provenance, .. }
            | Self::SetThreatClaim { provenance, .. }
            | Self::SetThreatScaling { provenance, .. }
            | Self::SetImpactRationale { provenance, .. }
            | Self::SetFeasibilityRationale { provenance, .. }
            | Self::SetDamageLinkRationale { provenance, .. }
            | Self::AddComponent { provenance, .. }
            | Self::RenameComponent { provenance, .. }
            | Self::SetComponentParent { provenance, .. }
            | Self::SetComponentSystem { provenance, .. }
            | Self::AddInterface { provenance, .. }
            | Self::RenameInterface { provenance, .. }
            | Self::SetInterfaceKind { provenance, .. }
            | Self::AddDataFlow { provenance, .. }
            | Self::RenameDataFlow { provenance, .. }
            | Self::SetDataFlowInterface { provenance, .. }
            | Self::AddTrustBoundary { provenance, .. }
            | Self::RenameTrustBoundary { provenance, .. }
            | Self::AddBoundaryComponent { provenance, .. }
            | Self::RemoveBoundaryComponent { provenance, .. }
            | Self::SetAssetComponent { provenance, .. } => {
                *provenance = new_provenance;
            }
            Self::RemoveAsset { .. }
            | Self::RemoveDamageScenario { .. }
            | Self::RemoveThreatScenario { .. }
            | Self::RemoveAttackTree { .. }
            | Self::RemoveSecurityControl { .. }
            | Self::RemoveCybersecurityGoal { .. }
            | Self::RemoveCybersecurityClaim { .. }
            | Self::RemoveParty { .. }
            | Self::RemoveCondition { .. }
            | Self::RemoveElement { .. }
            | Self::RemoveComponent { .. }
            | Self::RemoveInterface { .. }
            | Self::RemoveDataFlow { .. }
            | Self::RemoveTrustBoundary { .. } => {}
        }
        self
    }
}
