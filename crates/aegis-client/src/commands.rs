//! What the user can ask for.
//!
//! [`Command`] is the whole interface between a UI and the session. Edits
//! name entities by their stable IDs (taken from the rows the view model
//! offered) and vocabulary by *positions* in the lists the view model
//! offered (a dropdown index), never by model internals; [`Edit::to_ops`]
//! resolves them against the methodology and builds the typed `Op`s.
//!
//! Removals cascade the links that would block them, in the same edit, so
//! one undo brings everything back: removing a damage scenario unlinks it
//! from its threats first, removing a control detaches it from its steps and
//! goals first, and so on. An asset that threats target is not removed:
//! the user decides what happens to those threats.

use aegis_model::{
    AssetId, AttackTree, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GateKind, GoalId,
    InterfaceId, ItemRef, NodeId, Op, Project, PropertySet, Provenance, RiskTreatment, ThreatId, TreeId,
    TrustBoundaryId,
};

use crate::outline::{OutlineError, parse_outline};

/// The treatment decisions, in the order the view offers them.
pub const TREATMENTS: [RiskTreatment; 4] = [
    RiskTreatment::Avoid,
    RiskTreatment::Reduce,
    RiskTreatment::Share,
    RiskTreatment::Retain,
];

/// The attack tree gate kinds, in the order the view offers them (`OR`, `AND`, `SEQ`).
pub const GATES: [GateKind; 3] = [GateKind::Or, GateKind::And, GateKind::SeqAnd];

/// A change to the project, as the UI states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    /// Several edits as one: one undo step, and refused as a whole if any of
    /// them is (bulk edit, UI-04). Each is built against the project as it is
    /// before the batch, so an edit that needs another one of the batch first
    /// (removing an asset and the threats on it) is refused; remove those in
    /// the order the model needs.
    Batch(Vec<Edit>),
    // ---- descriptions and notes ----
    /// Sets the description of an element, or the notes of the project
    /// ([`ElementRef::Project`]). Empty text clears it.
    SetDescription {
        /// What the text belongs to.
        target: aegis_model::ElementRef,
        /// The text; line breaks are normalized to `\n`.
        text: String,
    },
    /// Sets or clears one translation of a description (FND-04).
    SetDescriptionTranslation {
        /// What the description belongs to.
        target: aegis_model::ElementRef,
        /// The language tag of the translation.
        lang: String,
        /// The translated text; empty clears it.
        text: String,
    },
    /// Sets the languages the project keeps translations in; empty makes
    /// it single-language. Tags are trimmed.
    SetProjectLanguages {
        /// The language tags, in display order.
        langs: Vec<String>,
    },

    // ---- attack tree steps ----
    /// Sets or clears the direct feasibility rating of a step.
    SetStepFeasibility {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the feasibility levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Sets or clears what a step goes through (CFG-01).
    SetStepVia {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// A component or an interface, or `None` for nothing.
        via: Option<ItemRef>,
    },
    /// Sets or clears the attack type of a step (CFG-02).
    SetStepAttackType {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the methodology's attack types, or `None`.
        attack_type: Option<usize>,
    },
    /// Sets or clears one attack potential factor of a step.
    SetStepFactor {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// Position in the methodology's factors.
        factor: usize,
        /// Position in that factor's levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Attaches a control to a step with a residual feasibility cap, changes
    /// the cap, or detaches it (`None`).
    SetStepMitigation {
        /// Tree.
        tree: TreeId,
        /// Step node.
        node: NodeId,
        /// The control.
        control: ControlId,
        /// Position in the feasibility levels, or `None` to detach.
        level: Option<usize>,
    },

    // ---- assets ----
    /// Adds an asset with the properties at these positions.
    AddAsset {
        /// New ID.
        id: AssetId,
        /// Name.
        name: String,
        /// Positions in the methodology's properties.
        properties: Vec<usize>,
    },
    /// Renames an asset.
    RenameAsset {
        /// The asset.
        asset: AssetId,
        /// New name.
        name: String,
    },
    /// Turns one security property of an asset on or off.
    SetAssetProperty {
        /// The asset.
        asset: AssetId,
        /// Position in the methodology's properties.
        property: usize,
        /// On or off.
        on: bool,
    },
    /// Binds an asset to a component in the item definition, or unbinds it.
    SetAssetComponent {
        /// The asset.
        asset: AssetId,
        /// The component, or `None`.
        component: Option<ComponentId>,
    },
    /// Removes an asset that no threat targets.
    RemoveAsset {
        /// The asset.
        asset: AssetId,
    },
    /// Adds an asset bound to a component (row-action context, UI-09).
    AddAssetBound {
        /// New ID.
        id: AssetId,
        /// Name.
        name: String,
        /// Positions in the methodology's properties.
        properties: Vec<usize>,
        /// The component to bind it to.
        component: ComponentId,
    },

    // ---- item definition (job 074) ----
    /// Adds a component to the item definition.
    AddComponent {
        /// New ID.
        id: ComponentId,
        /// Name.
        name: String,
        /// Optional parent component.
        parent: Option<ComponentId>,
    },
    /// Renames a component.
    RenameComponent {
        /// The component.
        component: ComponentId,
        /// New name.
        name: String,
    },
    /// Sets or clears the parent component of a component.
    SetComponentParent {
        /// The component.
        component: ComponentId,
        /// Parent component, or `None`.
        parent: Option<ComponentId>,
    },
    /// Removes a component, unbinding children, assets, trust boundaries,
    /// interfaces, and their data flows first.
    RemoveComponent {
        /// The component.
        component: ComponentId,
    },
    /// Adds an interface on a component.
    AddInterface {
        /// New ID.
        id: InterfaceId,
        /// Name.
        name: String,
        /// Owning component.
        component: ComponentId,
        /// Position in the methodology's interface kinds.
        kind: usize,
    },
    /// Renames an interface.
    RenameInterface {
        /// The interface.
        interface: InterfaceId,
        /// New name.
        name: String,
    },
    /// Changes an interface's kind.
    SetInterfaceKind {
        /// The interface.
        interface: InterfaceId,
        /// Position in the methodology's interface kinds.
        kind: usize,
    },
    /// Removes an interface, removing its data flows first.
    RemoveInterface {
        /// The interface.
        interface: InterfaceId,
    },
    /// Adds a data flow between two components.
    AddDataFlow {
        /// New ID.
        id: DataFlowId,
        /// Name.
        name: String,
        /// Source component.
        from: ComponentId,
        /// Destination component.
        to: ComponentId,
        /// Optional interface on `from` or `to` over which the flow travels.
        interface: Option<InterfaceId>,
    },
    /// Renames a data flow.
    RenameDataFlow {
        /// The data flow.
        data_flow: DataFlowId,
        /// New name.
        name: String,
    },
    /// Sets or clears the interface over which a data flow travels.
    SetDataFlowInterface {
        /// The data flow.
        data_flow: DataFlowId,
        /// The interface on `from` or `to`, or `None`.
        interface: Option<InterfaceId>,
    },
    /// Removes a data flow.
    RemoveDataFlow {
        /// The data flow.
        data_flow: DataFlowId,
    },
    /// Adds a trust boundary.
    AddTrustBoundary {
        /// New ID.
        id: TrustBoundaryId,
        /// Name.
        name: String,
    },
    /// Renames a trust boundary.
    RenameTrustBoundary {
        /// The trust boundary.
        boundary: TrustBoundaryId,
        /// New name.
        name: String,
    },
    /// Adds or removes a component as a member of a trust boundary.
    SetTrustBoundaryMember {
        /// The trust boundary.
        boundary: TrustBoundaryId,
        /// The component.
        component: ComponentId,
        /// Member or not.
        member: bool,
    },
    /// Removes a trust boundary.
    RemoveTrustBoundary {
        /// The trust boundary.
        boundary: TrustBoundaryId,
    },

    // ---- damage scenarios ----
    /// Adds an unrated damage scenario.
    AddDamage {
        /// New ID.
        id: DamageId,
        /// Name.
        name: String,
    },
    /// Renames a damage scenario.
    RenameDamage {
        /// The damage scenario.
        damage: DamageId,
        /// New name.
        name: String,
    },
    /// Sets or clears one impact rating.
    SetImpact {
        /// The damage scenario.
        damage: DamageId,
        /// Position in the impact categories.
        category: usize,
        /// Position in that category's levels, or `None` for unrated.
        level: Option<usize>,
    },
    /// Removes a damage scenario, unlinking it from its threats first.
    RemoveDamage {
        /// The damage scenario.
        damage: DamageId,
    },

    // ---- threat scenarios ----
    /// Adds a threat scenario on an asset.
    AddThreat {
        /// New ID.
        id: ThreatId,
        /// Name.
        name: String,
        /// The asset it targets.
        asset: AssetId,
        /// Position in the threat categories.
        category: usize,
    },
    /// Adds a threat scenario that leads to a damage scenario (UI-09).
    AddThreatLeading {
        /// New ID.
        id: ThreatId,
        /// Name.
        name: String,
        /// The asset it targets.
        asset: AssetId,
        /// Position in the threat categories.
        category: usize,
        /// The damage scenario it leads to.
        damage: DamageId,
    },
    /// Renames a threat scenario.
    RenameThreat {
        /// The threat.
        threat: ThreatId,
        /// The new name.
        name: String,
    },
    /// Changes a threat's category.
    SetThreatCategory {
        /// The threat.
        threat: ThreatId,
        /// Position in the threat categories.
        category: usize,
    },
    /// Links or unlinks a damage scenario.
    LinkDamage {
        /// The threat.
        threat: ThreatId,
        /// The damage scenario.
        damage: DamageId,
        /// Link or unlink.
        linked: bool,
    },
    /// Adds a catalog reference (for example UN R155 Annex 5 `4.1`).
    AddCatalogRef {
        /// The threat.
        threat: ThreatId,
        /// Position in the catalog schemes.
        scheme: usize,
        /// The reference text.
        text: String,
    },
    /// Removes the catalog reference at a position in the threat's list.
    RemoveCatalogRef {
        /// The threat.
        threat: ThreatId,
        /// Position in the threat's catalog refs.
        index: usize,
    },
    /// Sets or clears the direct feasibility rating of a threat.
    SetThreatFeasibility {
        /// The threat.
        threat: ThreatId,
        /// Position in the feasibility levels, or `None`.
        level: Option<usize>,
    },
    /// Links an attack tree to a threat, or unlinks it.
    SetThreatTree {
        /// The threat.
        threat: ThreatId,
        /// The tree, or `None`.
        tree: Option<TreeId>,
    },
    /// Sets or clears the treatment decision.
    SetTreatment {
        /// The threat.
        threat: ThreatId,
        /// Position in [`TREATMENTS`], or `None`.
        treatment: Option<usize>,
    },
    /// Links a cybersecurity goal to a threat, or unlinks it.
    SetThreatGoal {
        /// The threat.
        threat: ThreatId,
        /// The goal, or `None`.
        goal: Option<GoalId>,
    },
    /// Links a cybersecurity claim to a threat, or unlinks it.
    SetThreatClaim {
        /// The threat.
        threat: ThreatId,
        /// The claim, or `None`.
        claim: Option<ClaimId>,
    },
    /// Removes a threat scenario, clearing what blocks the removal first
    /// (tree link, ratings, treatment, goal, claim).
    RemoveThreat {
        /// The threat.
        threat: ThreatId,
    },

    // ---- attack trees ----
    /// Creates a tree with one OR gate, optionally linked to a threat.
    NewTree {
        /// New ID.
        id: TreeId,
        /// Text of the root gate.
        root: String,
        /// The threat to link it to.
        threat: Option<ThreatId>,
    },
    /// Replaces a tree's structure with an outline (see [`crate::outline`]).
    /// Nodes that keep their text keep their ratings, factors and controls.
    ReplaceTree {
        /// The tree.
        tree: TreeId,
        /// The outline.
        outline: String,
    },
    /// Removes a tree, unlinking threats and clearing factors and controls
    /// first.
    RemoveTree {
        /// The tree.
        tree: TreeId,
    },
    /// Renames a single node (root, intermediate gate or leaf step) in place,
    /// keeping its identity, children, ratings, factors and controls.
    RenameTreeNode {
        /// The tree.
        tree: TreeId,
        /// The node to rename.
        node: NodeId,
        /// New text.
        label: String,
    },
    /// Changes a gate node's logic (`OR`, `AND` or `SEQ`) in place, keeping
    /// its identity and children.
    SetTreeNodeGate {
        /// The tree.
        tree: TreeId,
        /// The gate node.
        node: NodeId,
        /// New gate kind.
        gate: GateKind,
    },
    /// Expands an attack tree at the end of `parent`: appends a new step
    /// (`gate: None`) or gate (`gate: Some(kind)`) as the last child of
    /// `parent`. If `parent` is currently a leaf step, `parent` is converted
    /// into an `OR` gate in place and `id` is added under it.
    AddTreeChild {
        /// The tree.
        tree: TreeId,
        /// The parent gate (or leaf step to expand into a gate).
        parent: NodeId,
        /// New child node ID.
        id: NodeId,
        /// `None` for a step, `Some(kind)` for a gate.
        gate: Option<GateKind>,
        /// Child text.
        label: String,
    },
    /// Branches a leaf step `node` into a gate of kind `gate` at its current
    /// position and creates a child step `id` under it, transferring any
    /// existing feasibility rating, attack-potential factors, security
    /// controls and review mark from `node` onto `id`.
    BranchStepToGate {
        /// The tree.
        tree: TreeId,
        /// The step node to convert into a gate.
        node: NodeId,
        /// New child step ID.
        id: NodeId,
        /// Gate kind for `node`.
        gate: GateKind,
        /// Text for the new child step `id`.
        child_label: String,
    },
    /// Expands an attack tree at an intermediate point by inserting a new gate
    /// `id` at `node`: for a non-root node, `id` takes `node`'s slot under its
    /// owner and wraps `node` (and its subtree) as `id`'s child; for the root
    /// gate, `id` wraps all of the root gate's current children under `id`.
    InsertParentGate {
        /// The tree.
        tree: TreeId,
        /// The node to wrap under the new intermediate gate.
        node: NodeId,
        /// New intermediate gate ID.
        id: NodeId,
        /// Gate kind for `id`.
        gate: GateKind,
        /// Text for the new intermediate gate `id`.
        label: String,
    },
    /// Adds a sibling step (`gate: None`) or gate (`gate: Some(kind)`) right
    /// after `node` under `node`'s owner gate.
    AddSiblingNode {
        /// The tree.
        tree: TreeId,
        /// Existing non-root node whose owner receives the new sibling.
        node: NodeId,
        /// New sibling node ID.
        id: NodeId,
        /// `None` for a step, `Some(kind)` for a gate.
        gate: Option<GateKind>,
        /// Text for the new sibling.
        label: String,
    },
    /// Moves a non-root node up (`delta = -1`) or down (`delta = 1`) among its
    /// owner gate's children.
    MoveTreeNode {
        /// The tree.
        tree: TreeId,
        /// The node to reorder.
        node: NodeId,
        /// Relative offset (`-1` or `+1`).
        delta: isize,
    },
    /// Removes a non-root node and any descendants reachable only through it
    /// (clearing step ratings, factors and controls first). Shared descendants
    /// that remain reachable via another gate are preserved.
    RemoveTreeNode {
        /// The tree.
        tree: TreeId,
        /// The node to remove.
        node: NodeId,
    },

    /// Copies a library item into the project (see [`crate::library`]).
    InsertLibraryItem {
        /// The pack.
        pack: std::sync::Arc<aegis_model::library::LibraryPack>,
        /// Position of the item in the pack.
        item: usize,
        /// Where it goes.
        mode: crate::library::InsertMode,
        /// Ratings and sharing.
        options: crate::library::InsertOptions,
    },
    /// Says a person has checked a rating that came from the library.
    MarkReviewed {
        /// The tree.
        tree: TreeId,
        /// The step.
        node: NodeId,
    },

    // ---- controls, goals, claims ----
    /// Adds a security control.
    AddControl {
        /// New ID.
        id: ControlId,
        /// Name.
        name: String,
    },
    /// Renames a security control.
    RenameControl {
        /// The control.
        control: ControlId,
        /// New name.
        name: String,
    },
    /// Removes a control, detaching it from steps and goals first.
    RemoveControl {
        /// The control.
        control: ControlId,
    },
    /// Adds a cybersecurity goal.
    AddGoal {
        /// New ID.
        id: GoalId,
        /// Name.
        name: String,
    },
    /// Adds a goal linked to a security control (row-action context, UI-09).
    AddGoalFor {
        /// New ID.
        id: GoalId,
        /// Name.
        name: String,
        /// The security control to link it to.
        control: ControlId,
    },
    /// Renames a cybersecurity goal.
    RenameGoal {
        /// The goal.
        goal: GoalId,
        /// New name.
        name: String,
    },
    /// Links or unlinks a control on a goal.
    LinkGoalControl {
        /// The goal.
        goal: GoalId,
        /// The control.
        control: ControlId,
        /// Link or unlink.
        linked: bool,
    },
    /// Removes a goal, unlinking it from threats first.
    RemoveGoal {
        /// The goal.
        goal: GoalId,
    },
    /// Adds a cybersecurity claim.
    AddClaim {
        /// New ID.
        id: ClaimId,
        /// Statement.
        name: String,
    },
    /// Renames a cybersecurity claim.
    RenameClaim {
        /// The claim.
        claim: ClaimId,
        /// New statement.
        name: String,
    },
    /// Removes a claim, unlinking it from threats first.
    RemoveClaim {
        /// The claim.
        claim: ClaimId,
    },

    // ---- duplication (UI-09.2, UI-09.4) ----
    /// Duplicates an element, cloning its own scalar fields with a fresh id
    /// and the name with the given suffix after it ("copy" in English). Cross-links (members, damages, controls,
    /// catalog refs, steps) are not cloned.
    DuplicateComponent {
        /// New ID.
        id: ComponentId,
        /// The component to copy.
        source: ComponentId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates an interface.
    DuplicateInterface {
        /// New ID.
        id: InterfaceId,
        /// The interface to copy.
        source: InterfaceId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a data flow.
    DuplicateDataFlow {
        /// New ID.
        id: DataFlowId,
        /// The data flow to copy.
        source: DataFlowId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a trust boundary (members are not copied).
    DuplicateTrustBoundary {
        /// New ID.
        id: TrustBoundaryId,
        /// The trust boundary to copy.
        source: TrustBoundaryId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates an asset (its component binding is not copied).
    DuplicateAsset {
        /// New ID.
        id: AssetId,
        /// The asset to copy.
        source: AssetId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a damage scenario (its rated impacts are copied).
    DuplicateDamage {
        /// New ID.
        id: DamageId,
        /// The damage scenario to copy.
        source: DamageId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a threat scenario (damages and catalog refs are not copied).
    DuplicateThreat {
        /// New ID.
        id: ThreatId,
        /// The threat scenario to copy.
        source: ThreatId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a security control.
    DuplicateControl {
        /// New ID.
        id: ControlId,
        /// The control to copy.
        source: ControlId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a cybersecurity goal (controls are not copied).
    DuplicateGoal {
        /// New ID.
        id: GoalId,
        /// The goal to copy.
        source: GoalId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
    /// Duplicates a cybersecurity claim.
    DuplicateClaim {
        /// New ID.
        id: ClaimId,
        /// The claim to copy.
        source: ClaimId,
        /// Word put after the name of the copy, in the user's language.
        suffix: String,
    },
}

/// Why an edit could not be turned into ops. The text is for the user.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The first line of an outline would turn the root from a gate into a
    /// step or the other way round.
    #[error("the first line must stay a gate (OR, AND, SEQ), or a step if the tree is one step")]
    RootKind,
    /// The feasibility level position does not exist.
    #[error("unknown feasibility level")]
    UnknownLevel,
    /// The factor position does not exist.
    #[error("unknown factor")]
    UnknownFactor,
    /// The factor level position does not exist.
    #[error("unknown factor level")]
    UnknownFactorLevel,
    /// A vocabulary position (property, category, scheme) does not exist.
    #[error("unknown choice")]
    UnknownChoice,
    /// The entity is gone (another editor removed it, or an undo).
    #[error("it no longer exists")]
    Missing,
    /// An empty name.
    #[error("the name is empty")]
    EmptyName,
    /// An asset needs at least one security property.
    #[error("an asset needs at least one security property")]
    NoProperties,
    /// The asset is still targeted by threats.
    #[error(
        "{0} threat scenario(s) still target this asset; remove them or change their asset first"
    )]
    AssetInUse(usize),
    /// The outline is not a tree.
    #[error("{0}")]
    Outline(#[from] OutlineError),
    /// The catalog reference does not parse.
    #[error("the reference does not match the catalog's format")]
    BadCatalogRef,
    /// A library item cannot go there.
    #[error("{0}")]
    Library(String),
}

/// Ops that clear the "through" of every step that goes through `item`
/// itself (a component's interfaces are not included).
pub(crate) fn clear_via_ops(project: &Project, item: ItemRef, p: &Provenance) -> Vec<Op> {
    project
        .steps_via(item)
        .filter_map(|(t_idx, n_idx)| {
            Some(Op::SetStepVia {
                tree: project.attack_tree_id(t_idx)?,
                node: project.attack_tree(t_idx)?.id_of(n_idx)?,
                via: None,
                provenance: p.clone(),
            })
        })
        .collect()
}

/// An element with a description cannot be removed, so every removal in an
/// edit is preceded by an op that clears its description. The edit is one
/// undo step, and undo brings back the element and then its text.
fn clear_descriptions_before_removal(
    project: &Project,
    ops: Vec<Op>,
    provenance: &Provenance,
) -> Vec<Op> {
    use aegis_model::ElementRef as E;
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let target = match &op {
            Op::RemoveComponent { id } => Some(E::Component(*id)),
            Op::RemoveInterface { id } => Some(E::Interface(*id)),
            Op::RemoveDataFlow { id } => Some(E::DataFlow(*id)),
            Op::RemoveTrustBoundary { id } => Some(E::TrustBoundary(*id)),
            Op::RemoveAsset { id } => Some(E::Asset(*id)),
            Op::RemoveDamageScenario { id } => Some(E::Damage(*id)),
            Op::RemoveThreatScenario { id } => Some(E::Threat(*id)),
            Op::RemoveAttackTree { id } => Some(E::Tree(*id)),
            Op::RemoveSecurityControl { id } => Some(E::Control(*id)),
            Op::RemoveCybersecurityGoal { id } => Some(E::Goal(*id)),
            Op::RemoveCybersecurityClaim { id } => Some(E::Claim(*id)),
            _ => None,
        };
        if let Some(target) = target.filter(|t| project.description(*t).is_some()) {
            for (lang, _) in project.description_translations(target) {
                out.push(Op::SetDescriptionTranslation {
                    target,
                    lang: lang.into(),
                    text: "".into(),
                    provenance: provenance.clone(),
                });
            }
            out.push(Op::SetDescription {
                target,
                text: "".into(),
                provenance: provenance.clone(),
            });
        }
        out.push(op);
    }
    out
}

fn name(text: &str) -> Result<std::sync::Arc<str>, EditError> {
    let t = text.trim();
    if t.is_empty() {
        Err(EditError::EmptyName)
    } else {
        Ok(t.into())
    }
}

/// The name a duplicated element gets (UI-09.2).
fn copy_name(text: &str, suffix: &str) -> String {
    format!("{} {}", text.trim(), suffix.trim())
}

fn feasibility(
    project: &Project,
    level: Option<usize>,
) -> Result<Option<aegis_model::FeasibilityLevel>, EditError> {
    level
        .map(|i| {
            project
                .methodology()
                .feasibility_levels()
                .nth(i)
                .map(|(l, _, _)| l)
                .ok_or(EditError::UnknownLevel)
        })
        .transpose()
}

/// The node without its children, for a tree reduced to its root.
fn root_node_without_children(node: &aegis_model::Node) -> aegis_model::Node {
    match node.gate_kind() {
        Some(kind) => aegis_model::Node::gate(kind, node.label()),
        None => aegis_model::Node::step(node.label()),
    }
}

fn threats_on_tree(project: &Project, tree: TreeId) -> Vec<ThreatId> {
    let Some(t_idx) = project.attack_tree_idx(&tree) else {
        return Vec::new();
    };
    project
        .threat_scenarios()
        .filter(|(_, _, t)| t.attack_tree() == Some(t_idx))
        .map(|(_, id, _)| id)
        .collect()
}

impl Edit {
    /// Builds the ops for this edit against `project` and its methodology.
    /// The provenance records who made the change and when.
    ///
    /// # Errors
    ///
    /// [`EditError`] if a position does not exist, an entity is gone, or
    /// the input is invalid. The model checks everything else when the ops
    /// are applied.
    #[allow(clippy::too_many_lines)]
    pub fn to_ops(
        &self,
        project: &Project,
        actor: &str,
        now_ms: u64,
    ) -> Result<Vec<Op>, EditError> {
        let m = project.methodology();
        let p = Provenance::human(actor, now_ms);
        let clearing = p.clone();
        let ops = match self {
            Edit::Batch(items) => {
                let mut ops = Vec::new();
                for item in items {
                    ops.extend(item.to_ops(project, actor, now_ms)?);
                }
                ops
            }
            Edit::SetDescription { target, text } => {
                let normalized: std::sync::Arc<str> =
                    text.replace("\r\n", "\n").replace('\r', "\n").into();
                let mut ops = Vec::new();
                if normalized.is_empty() {
                    for (lang, _) in project.description_translations(*target) {
                        ops.push(Op::SetDescriptionTranslation {
                            target: *target,
                            lang: lang.into(),
                            text: "".into(),
                            provenance: p.clone(),
                        });
                    }
                }
                ops.push(Op::SetDescription {
                    target: *target,
                    text: normalized,
                    provenance: p,
                });
                ops
            }
            Edit::SetDescriptionTranslation { target, lang, text } => {
                vec![Op::SetDescriptionTranslation {
                    target: *target,
                    lang: lang.trim().to_owned().into(),
                    text: text.replace("\r\n", "\n").replace('\r', "\n").into(),
                    provenance: p,
                }]
            }
            Edit::SetProjectLanguages { langs } => vec![Op::SetProjectLanguages {
                langs: langs.iter().map(|l| l.trim().into()).collect(),
                provenance: p,
            }],
            Edit::SetStepFeasibility { tree, node, level } => vec![Op::SetStepFeasibility {
                tree: *tree,
                node: *node,
                level: feasibility(project, *level)?,
                provenance: p,
            }],
            Edit::SetStepVia { tree, node, via } => vec![Op::SetStepVia {
                tree: *tree,
                node: *node,
                via: *via,
                provenance: p,
            }],
            Edit::SetStepAttackType {
                tree,
                node,
                attack_type,
            } => vec![Op::SetStepAttackType {
                tree: *tree,
                node: *node,
                attack_type: attack_type
                    .map(|i| m.attack_types().nth(i).ok_or(EditError::UnknownChoice))
                    .transpose()?,
                provenance: p,
            }],
            Edit::SetStepFactor {
                tree,
                node,
                factor,
                level,
            } => {
                let f = m
                    .feasibility_factors()
                    .nth(*factor)
                    .ok_or(EditError::UnknownFactor)?;
                let level = match level {
                    Some(i) => Some(
                        m.factor_levels(f)
                            .nth(*i)
                            .ok_or(EditError::UnknownFactorLevel)?,
                    ),
                    None => None,
                };
                vec![Op::SetStepFactor {
                    tree: *tree,
                    node: *node,
                    factor: f,
                    level,
                    provenance: p,
                }]
            }
            Edit::SetStepMitigation {
                tree,
                node,
                control,
                level,
            } => vec![Op::SetStepMitigation {
                tree: *tree,
                node: *node,
                control: *control,
                feasibility: feasibility(project, *level)?,
                provenance: p,
            }],

            Edit::AddAsset {
                id,
                name: n,
                properties,
            } => {
                let mut set = PropertySet::default();
                for &i in properties {
                    set = set.with(m.properties().nth(i).ok_or(EditError::UnknownChoice)?);
                }
                if set.is_empty() {
                    return Err(EditError::NoProperties);
                }
                vec![Op::AddAsset {
                    id: *id,
                    name: name(n)?,
                    properties: set,
                    provenance: p,
                }]
            }
            Edit::AddAssetBound {
                id,
                name: n,
                properties,
                component,
            } => {
                let mut set = PropertySet::default();
                for &i in properties {
                    set = set.with(m.properties().nth(i).ok_or(EditError::UnknownChoice)?);
                }
                if set.is_empty() {
                    return Err(EditError::NoProperties);
                }
                vec![
                    Op::AddAsset {
                        id: *id,
                        name: name(n)?,
                        properties: set,
                        provenance: p.clone(),
                    },
                    Op::SetAssetComponent {
                        asset: *id,
                        component: Some(*component),
                        provenance: p,
                    },
                ]
            }
            Edit::RenameAsset { asset, name: n } => vec![Op::RenameAsset {
                id: *asset,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetAssetProperty {
                asset,
                property,
                on,
            } => {
                let a = project
                    .asset_idx(asset)
                    .and_then(|i| project.asset(i))
                    .ok_or(EditError::Missing)?;
                let prop = m
                    .properties()
                    .nth(*property)
                    .ok_or(EditError::UnknownChoice)?;
                let set = if *on {
                    a.properties().with(prop)
                } else {
                    a.properties()
                        .iter()
                        .filter(|q| *q != prop)
                        .fold(PropertySet::default(), PropertySet::with)
                };
                if set.is_empty() {
                    return Err(EditError::NoProperties);
                }
                vec![Op::SetAssetProperties {
                    id: *asset,
                    properties: set,
                    provenance: p,
                }]
            }
            Edit::SetAssetComponent { asset, component } => vec![Op::SetAssetComponent {
                asset: *asset,
                component: *component,
                provenance: p,
            }],
            Edit::RemoveAsset { asset } => {
                let idx = project.asset_idx(asset).ok_or(EditError::Missing)?;
                let a = project.asset(idx).ok_or(EditError::Missing)?;
                let threats = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.asset() == idx)
                    .count();
                if threats > 0 {
                    return Err(EditError::AssetInUse(threats));
                }
                let mut ops = Vec::new();
                if a.component().is_some() {
                    ops.push(Op::SetAssetComponent {
                        asset: *asset,
                        component: None,
                        provenance: p.clone(),
                    });
                }
                ops.push(Op::RemoveAsset { id: *asset });
                ops
            }

            Edit::AddComponent {
                id,
                name: n,
                parent,
            } => vec![Op::AddComponent {
                id: *id,
                name: name(n)?,
                parent: *parent,
                provenance: p,
            }],
            Edit::RenameComponent { component, name: n } => vec![Op::RenameComponent {
                id: *component,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetComponentParent { component, parent } => vec![Op::SetComponentParent {
                id: *component,
                parent: *parent,
                provenance: p,
            }],
            Edit::RemoveComponent { component } => {
                let c_idx = project.component_idx(component).ok_or(EditError::Missing)?;
                let mut ops = Vec::new();
                // Steps that go through the component or its interfaces lose
                // their "through" first (CFG-01).
                ops.extend(clear_via_ops(project, ItemRef::Component(*component), &p));
                for (_, iface_id, iface) in project.interfaces() {
                    if iface.component() == c_idx {
                        ops.extend(clear_via_ops(project, ItemRef::Interface(iface_id), &p));
                    }
                }
                for (_, child_id, child) in project.components() {
                    if child.parent() == Some(c_idx) {
                        ops.push(Op::SetComponentParent {
                            id: child_id,
                            parent: None,
                            provenance: p.clone(),
                        });
                    }
                }
                for (_, asset_id, asset) in project.assets() {
                    if asset.component() == Some(c_idx) {
                        ops.push(Op::SetAssetComponent {
                            asset: asset_id,
                            component: None,
                            provenance: p.clone(),
                        });
                    }
                }
                let iface_indices: Vec<_> = project
                    .interfaces()
                    .filter(|(_, _, i)| i.component() == c_idx)
                    .map(|(idx, id, _)| (idx, id))
                    .collect();
                for (_, df_id, df) in project.data_flows() {
                    if df.from() == c_idx || df.to() == c_idx {
                        ops.push(Op::RemoveDataFlow { id: df_id });
                    } else if df
                        .interface()
                        .is_some_and(|i_idx| iface_indices.iter().any(|(idx, _)| *idx == i_idx))
                    {
                        ops.push(Op::SetDataFlowInterface {
                            id: df_id,
                            interface: None,
                            provenance: p.clone(),
                        });
                    }
                }
                for (_, iface_id) in iface_indices {
                    ops.push(Op::RemoveInterface { id: iface_id });
                }
                for (_, tb_id, tb) in project.trust_boundaries() {
                    if tb.contains(c_idx) {
                        ops.push(Op::RemoveBoundaryComponent {
                            boundary: tb_id,
                            component: *component,
                            provenance: p.clone(),
                        });
                    }
                }
                ops.push(Op::RemoveComponent { id: *component });
                ops
            }
            Edit::AddInterface {
                id,
                name: n,
                component,
                kind,
            } => vec![Op::AddInterface {
                id: *id,
                name: name(n)?,
                component: *component,
                kind: m
                    .interface_kinds()
                    .nth(*kind)
                    .ok_or(EditError::UnknownChoice)?,
                provenance: p,
            }],
            Edit::RenameInterface { interface, name: n } => vec![Op::RenameInterface {
                id: *interface,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetInterfaceKind { interface, kind } => vec![Op::SetInterfaceKind {
                id: *interface,
                kind: m
                    .interface_kinds()
                    .nth(*kind)
                    .ok_or(EditError::UnknownChoice)?,
                provenance: p,
            }],
            Edit::RemoveInterface { interface } => {
                let i_idx = project.interface_idx(interface).ok_or(EditError::Missing)?;
                let mut ops = clear_via_ops(project, ItemRef::Interface(*interface), &p);
                ops.extend(project
                    .data_flows()
                    .filter(|(_, _, df)| df.interface() == Some(i_idx))
                    .map(|(_, df_id, _)| Op::SetDataFlowInterface {
                        id: df_id,
                        interface: None,
                        provenance: p.clone(),
                    }));
                ops.push(Op::RemoveInterface { id: *interface });
                ops
            }
            Edit::AddDataFlow {
                id,
                name: n,
                from,
                to,
                interface,
            } => vec![Op::AddDataFlow {
                id: *id,
                name: name(n)?,
                from: *from,
                to: *to,
                interface: *interface,
                provenance: p,
            }],
            Edit::RenameDataFlow { data_flow, name: n } => vec![Op::RenameDataFlow {
                id: *data_flow,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetDataFlowInterface {
                data_flow,
                interface,
            } => vec![Op::SetDataFlowInterface {
                id: *data_flow,
                interface: *interface,
                provenance: p,
            }],
            Edit::RemoveDataFlow { data_flow } => {
                if project.data_flow_idx(data_flow).is_none() {
                    return Err(EditError::Missing);
                }
                vec![Op::RemoveDataFlow { id: *data_flow }]
            }
            Edit::AddTrustBoundary { id, name: n } => vec![Op::AddTrustBoundary {
                id: *id,
                name: name(n)?,
                components: Vec::new(),
                provenance: p,
            }],
            Edit::RenameTrustBoundary { boundary, name: n } => vec![Op::RenameTrustBoundary {
                id: *boundary,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetTrustBoundaryMember {
                boundary,
                component,
                member,
            } => vec![if *member {
                Op::AddBoundaryComponent {
                    boundary: *boundary,
                    component: *component,
                    provenance: p,
                }
            } else {
                Op::RemoveBoundaryComponent {
                    boundary: *boundary,
                    component: *component,
                    provenance: p,
                }
            }],
            Edit::RemoveTrustBoundary { boundary } => {
                if project.trust_boundary_idx(boundary).is_none() {
                    return Err(EditError::Missing);
                }
                vec![Op::RemoveTrustBoundary { id: *boundary }]
            }

            Edit::AddDamage { id, name: n } => vec![Op::AddDamageScenario {
                id: *id,
                name: name(n)?,
                impacts: Vec::new(),
                provenance: p,
            }],
            Edit::RenameDamage { damage, name: n } => vec![Op::RenameDamageScenario {
                id: *damage,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetImpact {
                damage,
                category,
                level,
            } => {
                let cat = m
                    .impact_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?;
                let level = level
                    .map(|i| m.levels(cat).nth(i).ok_or(EditError::UnknownLevel))
                    .transpose()?;
                vec![Op::SetImpact {
                    id: *damage,
                    category: cat,
                    level,
                    provenance: p,
                }]
            }
            Edit::RemoveDamage { damage } => {
                let d_idx = project
                    .damage_scenario_idx(damage)
                    .ok_or(EditError::Missing)?;
                let mut ops: Vec<Op> = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.damages().contains(&d_idx))
                    .map(|(_, threat, _)| Op::UnlinkDamage {
                        threat,
                        damage: *damage,
                        provenance: p.clone(),
                    })
                    .collect();
                ops.push(Op::RemoveDamageScenario { id: *damage });
                ops
            }

            Edit::AddThreat {
                id,
                name: n,
                asset,
                category,
            } => vec![Op::AddThreatScenario {
                id: *id,
                name: name(n)?,
                asset: *asset,
                category: m
                    .threat_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?,
                catalog_refs: Vec::new(),
                damages: Vec::new(),
                provenance: p,
            }],
            Edit::AddThreatLeading {
                id,
                name: n,
                asset,
                category,
                damage,
            } => vec![Op::AddThreatScenario {
                id: *id,
                name: name(n)?,
                asset: *asset,
                category: m
                    .threat_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?,
                catalog_refs: Vec::new(),
                damages: vec![*damage],
                provenance: p,
            }],
            Edit::RenameThreat { threat, name: n } => vec![Op::RenameThreatScenario {
                id: *threat,
                name: name(n)?,
                provenance: p,
            }],
            Edit::SetThreatCategory { threat, category } => vec![Op::SetThreatCategory {
                id: *threat,
                category: m
                    .threat_categories()
                    .nth(*category)
                    .ok_or(EditError::UnknownChoice)?,
                provenance: p,
            }],
            Edit::LinkDamage {
                threat,
                damage,
                linked,
            } => vec![if *linked {
                Op::LinkDamage {
                    threat: *threat,
                    damage: *damage,
                    provenance: p,
                }
            } else {
                Op::UnlinkDamage {
                    threat: *threat,
                    damage: *damage,
                    provenance: p,
                }
            }],
            Edit::AddCatalogRef {
                threat,
                scheme,
                text,
            } => {
                let s = m
                    .catalog_schemes()
                    .nth(*scheme)
                    .ok_or(EditError::UnknownChoice)?;
                let catalog_ref = m
                    .parse_catalog_ref(s, text.trim())
                    .map_err(|_| EditError::BadCatalogRef)?;
                vec![Op::AddCatalogRef {
                    threat: *threat,
                    catalog_ref,
                    provenance: p,
                }]
            }
            Edit::RemoveCatalogRef { threat, index } => {
                let t = project
                    .threat_scenario_idx(threat)
                    .and_then(|i| project.threat_scenario(i))
                    .ok_or(EditError::Missing)?;
                let catalog_ref = *t.catalog_refs().get(*index).ok_or(EditError::Missing)?;
                vec![Op::RemoveCatalogRef {
                    threat: *threat,
                    catalog_ref,
                    provenance: p,
                }]
            }
            Edit::SetThreatFeasibility { threat, level } => vec![Op::SetThreatFeasibility {
                threat: *threat,
                level: feasibility(project, *level)?,
                provenance: p,
            }],
            Edit::SetThreatTree { threat, tree } => vec![Op::SetThreatAttackTree {
                threat: *threat,
                tree: *tree,
                provenance: p,
            }],
            Edit::SetTreatment { threat, treatment } => vec![Op::SetRiskTreatment {
                threat: *threat,
                treatment: treatment
                    .map(|i| TREATMENTS.get(i).copied().ok_or(EditError::UnknownChoice))
                    .transpose()?,
                provenance: p,
            }],
            Edit::SetThreatGoal { threat, goal } => vec![Op::SetThreatGoal {
                threat: *threat,
                goal: *goal,
                provenance: p,
            }],
            Edit::SetThreatClaim { threat, claim } => vec![Op::SetThreatClaim {
                threat: *threat,
                claim: *claim,
                provenance: p,
            }],
            Edit::RemoveThreat { threat } => {
                let t = project
                    .threat_scenario_idx(threat)
                    .and_then(|i| project.threat_scenario(i))
                    .ok_or(EditError::Missing)?;
                let mut ops = Vec::new();
                if t.attack_tree().is_some() {
                    ops.push(Op::SetThreatAttackTree {
                        threat: *threat,
                        tree: None,
                        provenance: p.clone(),
                    });
                }
                if t.direct_feasibility().is_some() {
                    ops.push(Op::SetThreatFeasibility {
                        threat: *threat,
                        level: None,
                        provenance: p.clone(),
                    });
                }
                if t.treatment().is_some() {
                    ops.push(Op::SetRiskTreatment {
                        threat: *threat,
                        treatment: None,
                        provenance: p.clone(),
                    });
                }
                if t.goal().is_some() {
                    ops.push(Op::SetThreatGoal {
                        threat: *threat,
                        goal: None,
                        provenance: p.clone(),
                    });
                }
                if t.claim().is_some() {
                    ops.push(Op::SetThreatClaim {
                        threat: *threat,
                        claim: None,
                        provenance: p.clone(),
                    });
                }
                ops.push(Op::RemoveThreatScenario { id: *threat });
                ops
            }

            Edit::NewTree { id, root, threat } => {
                let tree = AttackTree::new(
                    NodeId::new_v7(),
                    aegis_model::Node::gate(aegis_model::GateKind::Or, &*name(root)?),
                )
                .map_err(|_| EditError::EmptyName)?;
                let mut ops = vec![Op::AddAttackTree {
                    id: *id,
                    tree,
                    step_feasibility: Vec::new(),
                    provenance: p.clone(),
                }];
                if let Some(threat) = threat {
                    ops.push(Op::SetThreatAttackTree {
                        threat: *threat,
                        tree: Some(*id),
                        provenance: p,
                    });
                }
                ops
            }
            Edit::ReplaceTree { tree, outline } => {
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let new = parse_outline(outline, Some(old))?;
                crate::tree_diff::replace_tree_ops(project, *tree, &new, &p)?
            }
            Edit::RemoveTree { tree } => {
                if project.attack_tree_idx(tree).is_none() {
                    return Err(EditError::Missing);
                }
                let mut ops: Vec<Op> = threats_on_tree(project, *tree)
                    .into_iter()
                    .map(|threat| Op::SetThreatAttackTree {
                        threat,
                        tree: None,
                        provenance: p.clone(),
                    })
                    .collect();
                // Take the tree apart node by node: an op carries a whole tree
                // only up to MAX_TREE_OP_NODES nodes.
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let root = old.root();
                let root_id = old.id_of(root).ok_or(EditError::Missing)?;
                let root_node = old.get(root).ok_or(EditError::Missing)?;
                let bare = AttackTree::new(root_id, root_node_without_children(root_node))
                    .map_err(|_| EditError::Missing)?;
                ops.extend(crate::tree_diff::replace_tree_ops(
                    project, *tree, &bare, &p,
                )?);
                if root_node.gate_kind().is_none() {
                    ops.extend(crate::tree_diff::clear_step_ops(
                        project, *tree, root_id, &p,
                    ));
                }
                ops.push(Op::RemoveAttackTree { id: *tree });
                ops
            }
            Edit::RenameTreeNode { tree, node, label } => {
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                if old.idx_of(node).is_none() {
                    return Err(EditError::Missing);
                }
                vec![Op::RenameTreeNode {
                    tree: *tree,
                    id: *node,
                    label: name(label)?,
                    provenance: p,
                }]
            }
            Edit::SetTreeNodeGate { tree, node, gate } => {
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let idx = old.idx_of(node).ok_or(EditError::Missing)?;
                if old
                    .get(idx)
                    .and_then(aegis_model::Node::gate_kind)
                    .is_none()
                {
                    return Err(EditError::Missing);
                }
                vec![Op::SetTreeNodeGate {
                    tree: *tree,
                    id: *node,
                    gate: *gate,
                    provenance: p,
                }]
            }
            Edit::AddTreeChild {
                tree,
                parent,
                id,
                gate,
                label,
            } => {
                let label = name(label)?;
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let p_idx = old.idx_of(parent).ok_or(EditError::Missing)?;
                let p_node = old.get(p_idx).ok_or(EditError::Missing)?;
                if p_node.gate_kind().is_some() {
                    let node = match gate {
                        Some(k) => aegis_model::Node::gate(*k, label),
                        None => aegis_model::Node::step(label),
                    };
                    vec![Op::AddTreeNode {
                        tree: *tree,
                        parent: *parent,
                        id: *id,
                        node,
                        index: u32::try_from(old.len()).unwrap_or(u32::MAX),
                        position: u32::try_from(old.children(p_idx).len()).unwrap_or(u32::MAX),
                        provenance: p,
                    }]
                } else {
                    crate::tree_diff::branch_step_ops(
                        project,
                        *tree,
                        *parent,
                        *id,
                        GateKind::Or,
                        *gate,
                        label,
                        false,
                        &p,
                    )?
                }
            }
            Edit::BranchStepToGate {
                tree,
                node,
                id,
                gate,
                child_label,
            } => crate::tree_diff::branch_step_ops(
                project,
                *tree,
                *node,
                *id,
                *gate,
                None,
                name(child_label)?,
                true,
                &p,
            )?,
            Edit::InsertParentGate {
                tree,
                node,
                id,
                gate,
                label,
            } => crate::tree_diff::insert_parent_gate_ops(
                project,
                *tree,
                *node,
                *id,
                *gate,
                name(label)?,
                &p,
            )?,
            Edit::AddSiblingNode {
                tree,
                node,
                id,
                gate,
                label,
            } => {
                let label = name(label)?;
                let old = project
                    .attack_tree_idx(tree)
                    .and_then(|i| project.attack_tree(i))
                    .ok_or(EditError::Missing)?;
                let n_idx = old.idx_of(node).ok_or(EditError::Missing)?;
                let owner_idx = old.owner(n_idx).ok_or(EditError::Missing)?;
                let owner_id = old.id_of(owner_idx).ok_or(EditError::Missing)?;
                let pos = old
                    .children(owner_idx)
                    .iter()
                    .position(|c| *c == n_idx)
                    .map_or(old.children(owner_idx).len(), |i| i + 1);
                let new_node = match gate {
                    Some(k) => aegis_model::Node::gate(*k, label),
                    None => aegis_model::Node::step(label),
                };
                vec![Op::AddTreeNode {
                    tree: *tree,
                    parent: owner_id,
                    id: *id,
                    node: new_node,
                    index: u32::try_from(old.len()).unwrap_or(u32::MAX),
                    position: u32::try_from(pos).unwrap_or(u32::MAX),
                    provenance: p,
                }]
            }
            Edit::MoveTreeNode { tree, node, delta } => {
                crate::tree_diff::move_node_ops(project, *tree, *node, *delta, &p)?
            }
            Edit::RemoveTreeNode { tree, node } => {
                crate::tree_diff::remove_node_ops(project, *tree, *node, &p)?
            }

            Edit::InsertLibraryItem {
                pack,
                item,
                mode,
                options,
            } => crate::library::insert_ops(project, pack, *item, mode, options, actor, now_ms)?,
            Edit::MarkReviewed { tree, node } => vec![Op::SetReviewMark {
                tree: *tree,
                node: *node,
                marked: false,
                provenance: p,
            }],
            Edit::AddControl { id, name: n } => vec![Op::AddSecurityControl {
                id: *id,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RenameControl { control, name: n } => vec![Op::RenameSecurityControl {
                id: *control,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RemoveControl { control } => {
                let c_idx = project
                    .security_control_idx(control)
                    .ok_or(EditError::Missing)?;
                let mut ops = Vec::new();
                for (t_idx, tree_id, tree) in project.attack_trees() {
                    for (n_idx, node_id, _) in tree.iter() {
                        if project
                            .step_mitigations(t_idx, n_idx)
                            .iter()
                            .any(|s| s.control() == c_idx)
                        {
                            ops.push(Op::SetStepMitigation {
                                tree: tree_id,
                                node: node_id,
                                control: *control,
                                feasibility: None,
                                provenance: p.clone(),
                            });
                        }
                    }
                }
                for (_, goal_id, goal) in project.cybersecurity_goals() {
                    if goal.controls().contains(&c_idx) {
                        ops.push(Op::UnlinkGoalControl {
                            goal: goal_id,
                            control: *control,
                            provenance: p.clone(),
                        });
                    }
                }
                ops.push(Op::RemoveSecurityControl { id: *control });
                ops
            }
            Edit::AddGoal { id, name: n } => vec![Op::AddCybersecurityGoal {
                id: *id,
                name: name(n)?,
                controls: Vec::new(),
                provenance: p,
            }],
            Edit::AddGoalFor {
                id,
                name: n,
                control,
            } => vec![Op::AddCybersecurityGoal {
                id: *id,
                name: name(n)?,
                controls: vec![*control],
                provenance: p,
            }],
            Edit::RenameGoal { goal, name: n } => vec![Op::RenameCybersecurityGoal {
                id: *goal,
                name: name(n)?,
                provenance: p,
            }],
            Edit::LinkGoalControl {
                goal,
                control,
                linked,
            } => vec![if *linked {
                Op::LinkGoalControl {
                    goal: *goal,
                    control: *control,
                    provenance: p,
                }
            } else {
                Op::UnlinkGoalControl {
                    goal: *goal,
                    control: *control,
                    provenance: p,
                }
            }],
            Edit::RemoveGoal { goal } => {
                let g_idx = project
                    .cybersecurity_goal_idx(goal)
                    .ok_or(EditError::Missing)?;
                let mut ops: Vec<Op> = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.goal() == Some(g_idx))
                    .map(|(_, threat, _)| Op::SetThreatGoal {
                        threat,
                        goal: None,
                        provenance: p.clone(),
                    })
                    .collect();
                ops.push(Op::RemoveCybersecurityGoal { id: *goal });
                ops
            }
            Edit::AddClaim { id, name: n } => vec![Op::AddCybersecurityClaim {
                id: *id,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RenameClaim { claim, name: n } => vec![Op::RenameCybersecurityClaim {
                id: *claim,
                name: name(n)?,
                provenance: p,
            }],
            Edit::RemoveClaim { claim } => {
                let c_idx = project
                    .cybersecurity_claim_idx(claim)
                    .ok_or(EditError::Missing)?;
                let mut ops: Vec<Op> = project
                    .threat_scenarios()
                    .filter(|(_, _, t)| t.claim() == Some(c_idx))
                    .map(|(_, threat, _)| Op::SetThreatClaim {
                        threat,
                        claim: None,
                        provenance: p.clone(),
                    })
                    .collect();
                ops.push(Op::RemoveCybersecurityClaim { id: *claim });
                ops
            }
            Edit::DuplicateComponent { id, source, suffix } => {
                let c = project
                    .component_idx(source)
                    .and_then(|i| project.component(i))
                    .ok_or(EditError::Missing)?;
                let parent = c.parent().and_then(|i| project.component_id(i));
                vec![Op::AddComponent {
                    id: *id,
                    name: name(&copy_name(c.name(), suffix))?,
                    parent,
                    provenance: p,
                }]
            }
            Edit::DuplicateInterface { id, source, suffix } => {
                let i = project
                    .interface_idx(source)
                    .and_then(|x| project.interface(x))
                    .ok_or(EditError::Missing)?;
                let component = project
                    .component_id(i.component())
                    .ok_or(EditError::Missing)?;
                vec![Op::AddInterface {
                    id: *id,
                    name: name(&copy_name(i.name(), suffix))?,
                    component,
                    kind: i.kind(),
                    provenance: p,
                }]
            }
            Edit::DuplicateDataFlow { id, source, suffix } => {
                let df = project
                    .data_flow_idx(source)
                    .and_then(|x| project.data_flow(x))
                    .ok_or(EditError::Missing)?;
                let from = project.component_id(df.from()).ok_or(EditError::Missing)?;
                let to = project.component_id(df.to()).ok_or(EditError::Missing)?;
                let interface = df.interface().and_then(|x| project.interface_id(x));
                vec![Op::AddDataFlow {
                    id: *id,
                    name: name(&copy_name(df.name(), suffix))?,
                    from,
                    to,
                    interface,
                    provenance: p,
                }]
            }
            Edit::DuplicateTrustBoundary { id, source, suffix } => {
                let tb = project
                    .trust_boundary_idx(source)
                    .and_then(|x| project.trust_boundary(x))
                    .ok_or(EditError::Missing)?;
                vec![Op::AddTrustBoundary {
                    id: *id,
                    name: name(&copy_name(tb.name(), suffix))?,
                    components: Vec::new(),
                    provenance: p,
                }]
            }
            Edit::DuplicateAsset { id, source, suffix } => {
                let a = project
                    .asset_idx(source)
                    .and_then(|x| project.asset(x))
                    .ok_or(EditError::Missing)?;
                let properties = a.properties();
                if properties.is_empty() {
                    return Err(EditError::NoProperties);
                }
                vec![Op::AddAsset {
                    id: *id,
                    name: name(&copy_name(a.name(), suffix))?,
                    properties,
                    provenance: p,
                }]
            }
            Edit::DuplicateDamage { id, source, suffix } => {
                let d = project
                    .damage_scenario_idx(source)
                    .and_then(|x| project.damage_scenario(x))
                    .ok_or(EditError::Missing)?;
                let impacts: Vec<_> = d
                    .impacts()
                    .filter_map(|(category, level)| level.map(|l| (category, l)))
                    .collect();
                vec![Op::AddDamageScenario {
                    id: *id,
                    name: name(&copy_name(d.name(), suffix))?,
                    impacts,
                    provenance: p,
                }]
            }
            Edit::DuplicateThreat { id, source, suffix } => {
                let t = project
                    .threat_scenario_idx(source)
                    .and_then(|x| project.threat_scenario(x))
                    .ok_or(EditError::Missing)?;
                let asset = project.asset_id(t.asset()).ok_or(EditError::Missing)?;
                vec![Op::AddThreatScenario {
                    id: *id,
                    name: name(&copy_name(t.name(), suffix))?,
                    asset,
                    category: t.category(),
                    catalog_refs: Vec::new(),
                    damages: Vec::new(),
                    provenance: p,
                }]
            }
            Edit::DuplicateControl { id, source, suffix } => {
                let c = project
                    .security_control_idx(source)
                    .and_then(|x| project.security_control(x))
                    .ok_or(EditError::Missing)?;
                vec![Op::AddSecurityControl {
                    id: *id,
                    name: name(&copy_name(c.name(), suffix))?,
                    provenance: p,
                }]
            }
            Edit::DuplicateGoal { id, source, suffix } => {
                let g = project
                    .cybersecurity_goal_idx(source)
                    .and_then(|x| project.cybersecurity_goal(x))
                    .ok_or(EditError::Missing)?;
                vec![Op::AddCybersecurityGoal {
                    id: *id,
                    name: name(&copy_name(g.name(), suffix))?,
                    controls: Vec::new(),
                    provenance: p,
                }]
            }
            Edit::DuplicateClaim { id, source, suffix } => {
                let c = project
                    .cybersecurity_claim_idx(source)
                    .and_then(|x| project.cybersecurity_claim(x))
                    .ok_or(EditError::Missing)?;
                vec![Op::AddCybersecurityClaim {
                    id: *id,
                    name: name(&copy_name(c.name(), suffix))?,
                    provenance: p,
                }]
            }
        };
        Ok(clear_descriptions_before_removal(project, ops, &clearing))
    }

    /// The undo label of a bulk edit: "remove 3 assets" (UI-04).
    #[must_use]
    pub fn bulk_label(&self) -> Option<crate::undo_label::Label> {
        use crate::undo_label::Label;
        let Edit::Batch(items) = self else {
            return None;
        };
        let (verb, noun) = match items.first()? {
            Edit::RemoveAsset { .. } => ("remove", "assets"),
            Edit::RemoveDamage { .. } => ("remove", "damage scenarios"),
            Edit::RemoveThreat { .. } => ("remove", "threat scenarios"),
            Edit::RemoveControl { .. } => ("remove", "security controls"),
            Edit::SetTreatment { .. } => ("set treatment of", "threat scenarios"),
            _ => ("change", "items"),
        };
        Some(Label::bulk(verb, noun, items.len()))
    }

    /// The entity this edit creates, so the session can select it.
    #[must_use]
    pub fn created(&self) -> Option<crate::view::Selection> {
        use crate::view::Selection;
        match self {
            Edit::AddComponent { id, .. } => Some(Selection::Component(*id)),
            Edit::AddInterface { id, .. } => Some(Selection::Interface(*id)),
            Edit::AddDataFlow { id, .. } => Some(Selection::DataFlow(*id)),
            Edit::AddTrustBoundary { id, .. } => Some(Selection::TrustBoundary(*id)),
            Edit::AddAsset { id, .. } | Edit::AddAssetBound { id, .. } => {
                Some(Selection::Asset(*id))
            }
            Edit::AddDamage { id, .. } => Some(Selection::Damage(*id)),
            Edit::AddThreat { id, .. } | Edit::AddThreatLeading { id, .. } => {
                Some(Selection::Threat(*id))
            }
            Edit::NewTree { id, .. } => Some(Selection::Tree(*id)),
            Edit::AddTreeChild { tree, id, .. }
            | Edit::BranchStepToGate { tree, id, .. }
            | Edit::InsertParentGate { tree, id, .. }
            | Edit::AddSiblingNode { tree, id, .. } => Some(Selection::TreeNode(*tree, *id)),
            Edit::InsertLibraryItem {
                mode: crate::library::InsertMode::NewTree { id, .. },
                ..
            } => Some(Selection::Tree(*id)),
            Edit::AddControl { id, .. } => Some(Selection::Control(*id)),
            Edit::AddGoal { id, .. } | Edit::AddGoalFor { id, .. } => Some(Selection::Goal(*id)),
            Edit::AddClaim { id, .. } => Some(Selection::Claim(*id)),
            Edit::DuplicateComponent { id, .. } => Some(Selection::Component(*id)),
            Edit::DuplicateInterface { id, .. } => Some(Selection::Interface(*id)),
            Edit::DuplicateDataFlow { id, .. } => Some(Selection::DataFlow(*id)),
            Edit::DuplicateTrustBoundary { id, .. } => Some(Selection::TrustBoundary(*id)),
            Edit::DuplicateAsset { id, .. } => Some(Selection::Asset(*id)),
            Edit::DuplicateDamage { id, .. } => Some(Selection::Damage(*id)),
            Edit::DuplicateThreat { id, .. } => Some(Selection::Threat(*id)),
            Edit::DuplicateControl { id, .. } => Some(Selection::Control(*id)),
            Edit::DuplicateGoal { id, .. } => Some(Selection::Goal(*id)),
            Edit::DuplicateClaim { id, .. } => Some(Selection::Claim(*id)),
            _ => None,
        }
    }
}

/// Everything a UI can tell the session to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Connect again (after a login, or after the server was unreachable).
    Reconnect,
    /// Reload the project list.
    Refresh,
    /// Create an empty project and open it.
    CreateProject {
        /// Display name.
        name: String,
        /// Methodology key.
        methodology_key: String,
    },
    /// Open a project (read-only until [`Command::BeginEdit`]).
    OpenProject {
        /// The project ID as listed.
        id: String,
    },
    /// Close the open project.
    CloseProject,
    /// Start a draft of a new element: the page opens, the inspector shows the
    /// draft form (UI-08). Nothing is sent to the server.
    BeginDraft(crate::view::DraftKind),
    /// Create the element of the draft with this name as one edit. An empty
    /// name discards the draft and sends nothing. With `next`, a new draft
    /// starts at once. If the edit is refused, the draft stays.
    CommitDraft {
        /// The name typed in the draft.
        name: String,
        /// Start the next draft after creating this one.
        next: bool,
    },
    /// Discard the draft; nothing was sent, nothing is in the history.
    CancelDraft,
    /// Choose the asset of a threat draft.
    SetDraftAsset(aegis_model::AssetId),
    /// Choose the category of a threat draft (a position in the vocabulary).
    SetDraftCategory(usize),
    /// Choose the component of an interface draft, or the "from" end of a flow.
    SetDraftComponent(aegis_model::ComponentId),
    /// Choose the "to" end of a data flow draft.
    SetDraftTo(aegis_model::ComponentId),
    /// Choose the kind of an interface draft (a position in the vocabulary).
    SetDraftInterfaceKind(usize),
    /// Show a page of the open project.
    ShowPage(crate::view::Page),
    /// Select an entity (and show the page it belongs to when `navigate`).
    Select {
        /// What to select.
        selection: crate::view::Selection,
        /// Also switch to the entity's page.
        navigate: bool,
    },
    /// Show the tree of the threat at this position in the threat list.
    SelectThreat(usize),
    /// Select the node at this position in the scene, or none.
    SelectNode(Option<usize>),
    /// Show the residual (after controls) evaluation instead of the inherent one.
    ShowResidual(bool),
    /// Reload the open project from the server (a read-only view whose
    /// project has a newer version).
    Reload,
    /// Take the edit lock.
    BeginEdit,
    /// Finish pending edits and give the edit lock up.
    EndEdit,
    /// Change the project.
    Edit(Edit),
    /// Undo the last edit.
    Undo,
    /// Undo the newest `n` edits as one (the history panel, UI-03).
    UndoTo(usize),
    /// Redo the last undone edit.
    Redo,
    /// Fill an empty project with a small example TARA.
    CreateSample,
    /// Rename the open project (needs the edit lock).
    RenameProject(String),
    /// Delete a project (admin).
    DeleteProject(String),
    /// Load the open project's baselines.
    LoadBaselines,
    /// Freeze the open project's saved state as a baseline.
    CreateBaseline(String),
    /// Compare the current state with a baseline (by ID), or stop comparing.
    CompareWith(Option<String>),
    /// Load the open project's recent changes.
    LoadHistory,
    /// Write a report or table of the open project to a file.
    Export {
        /// What to write.
        kind: crate::report::ExportKind,
        /// Where.
        path: std::path::PathBuf,
        /// The report profile: sections, filters, language and branding
        /// (RPT-02 to RPT-04). Boxed so the command stays small on the
        /// channel.
        profile: Box<crate::report::ReportProfile>,
    },
    /// Admin: load the users.
    LoadUsers,
    /// Admin: load the server's event log (logins, exports, admin actions).
    LoadEvents,
    /// Admin: load the open sessions (password login).
    LoadSessions,
    /// Admin: create, change or delete a user.
    Admin(crate::view::AdminAction),
    /// Project access: load the open project's membership rows.
    LoadMemberships,
    /// Project access: add or change a membership row.
    SetMembership {
        /// `user:<subject>` or `group:<name>`.
        member: String,
        /// One of the six built-in role keys.
        role: String,
    },
    /// Project access: remove a membership row.
    RemoveMembership {
        /// `user:<subject>` or `group:<name>`.
        member: String,
    },
    /// Admin: break the open project's edit lock.
    BreakLock,
    /// Admin: check the open project's audit chain.
    VerifyAudit,
    /// Change the user's own password.
    ChangePassword {
        /// The current password.
        current: String,
        /// The new password.
        new: String,
    },
    /// End the session on the server; the UI then shows the login.
    LogOut,
    /// Remove a notice from the list.
    DismissNotice(u64),
    /// Fetch the signed update manifest and say whether a newer release
    /// exists (the answer is a notice).
    CheckForUpdates,
    /// Download the installer of the release the last check offered, check
    /// its size and SHA-256 against the signed manifest, and say where it is
    /// (the answer is a notice). Nothing is run.
    DownloadUpdate,
    /// Synthesizes predictive ghost attack-tree steps for the shown tree (or
    /// selected node) using the local Aegis-Core Gear 1 engine (job 089).
    SuggestTreeNodes,
    /// Accepts one pending suggestion by its position in
    /// [`crate::state::ProjectView::suggestions`].
    AcceptSuggestion(usize),
    /// Accepts all remaining pending suggestions in the active branch.
    AcceptAllSuggestions,
    /// Rejects one pending suggestion by its position in
    /// [`crate::state::ProjectView::suggestions`].
    RejectSuggestion(usize),
    /// Rejects all pending suggestions in the active branch.
    RejectAllSuggestions,
    /// Loads figure attachments of the open project (job 073).
    LoadFigures,
    /// Uploads a figure attachment on the open project (job 073).
    PutFigure {
        /// Caption text.
        caption: String,
        /// Target string (`""` or `"project"` or `<kind>:<uuidv7>`).
        target: String,
        /// Raw image bytes (PNG, JPEG, or SVG).
        data: Vec<u8>,
    },
    /// Updates the caption and target of an existing figure attachment (job 073).
    UpdateFigure {
        /// Stable UUIDv7 figure identifier.
        figure_id: String,
        /// Updated caption text.
        caption: String,
        /// Updated target string.
        target: String,
    },
    /// Deletes a figure attachment from the open project (job 073).
    DeleteFigure {
        /// Stable UUIDv7 figure identifier.
        figure_id: String,
    },
    /// Runs a dry-run preview of an Excel `.xlsx` TARA import (job 080).
    PreviewExcelImport {
        /// Raw `.xlsx` workbook bytes.
        bytes: Vec<u8>,
        /// Column-to-element mapping.
        mapping: Box<crate::excel::ExcelMapping>,
    },
    /// Applies the last dry-run Excel import preview as one atomic batch with
    /// `Provenance::import` (job 080).
    ApplyExcelImport,
    /// Clears the current Excel import preview.
    ClearExcelPreview,
    /// Stop the session.
    Shutdown,
}
