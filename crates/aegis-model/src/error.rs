//! Typed errors for the model crate.

use std::fmt;
use std::sync::Arc;

use crate::control::RiskTreatment;
use crate::id::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, GoalId, InterfaceId,
    NodeId, NodeIdx, ThreatId, TreeId, TrustBoundaryId,
};
use crate::item::ItemRef;
use crate::methodology::{
    AttackType, CatalogRef, CatalogScheme, CatalogSyntax, FactorLevel, FeasibilityFactor,
    FeasibilityLevel, ImpactCategory, InterfaceKind, Level, Property, RiskLevel, ThreatCategory,
};
use crate::store::{Limit, VocabKind};

/// Which entity kind a [`ModelError`] refers to.
///
/// Used by [`ModelError::EmptyName`], [`ModelError::NameTooLong`], and
/// [`ModelError::ProjectCapacityExceeded`] so the error says which kind of
/// entity had a bad name or hit its slot limit without naming the entity
/// itself.
///
/// ```
/// use aegis_model::EntityKind;
///
/// assert_eq!(EntityKind::Asset.to_string(), "asset");
/// assert_eq!(EntityKind::DamageScenario.to_string(), "damage scenario");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EntityKind {
    /// An item definition component ([`Component`](crate::Component)).
    Component,
    /// An item definition interface ([`Interface`](crate::Interface)).
    Interface,
    /// An item definition data flow ([`DataFlow`](crate::DataFlow)).
    DataFlow,
    /// An item definition trust boundary ([`TrustBoundary`](crate::TrustBoundary)).
    TrustBoundary,
    /// An asset ([`Asset`](crate::Asset)).
    Asset,
    /// A damage scenario ([`DamageScenario`](crate::DamageScenario)).
    DamageScenario,
    /// A threat scenario ([`ThreatScenario`](crate::ThreatScenario)).
    ThreatScenario,
    /// An attack tree ([`AttackTree`](crate::AttackTree)).
    AttackTree,
    /// A node in an attack tree ([`Node`](crate::Node)).
    TreeNode,
    /// A security control ([`SecurityControl`](crate::SecurityControl)).
    SecurityControl,
    /// A cybersecurity goal ([`CybersecurityGoal`](crate::CybersecurityGoal)).
    CybersecurityGoal,
    /// A cybersecurity claim ([`CybersecurityClaim`](crate::CybersecurityClaim)).
    CybersecurityClaim,
    /// An element of a generic kind ([`Element`](crate::Element)).
    Element,
}

impl EntityKind {
    /// Returns the lowercase human-readable name of this entity kind.
    ///
    /// ```
    /// use aegis_model::EntityKind;
    ///
    /// assert_eq!(EntityKind::Asset.as_str(), "asset");
    /// assert_eq!(EntityKind::TreeNode.as_str(), "tree node");
    /// assert_eq!(EntityKind::SecurityControl.as_str(), "security control");
    /// assert_eq!(EntityKind::CybersecurityGoal.as_str(), "cybersecurity goal");
    /// assert_eq!(EntityKind::CybersecurityClaim.as_str(), "cybersecurity claim");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Interface => "interface",
            Self::DataFlow => "data flow",
            Self::TrustBoundary => "trust boundary",
            Self::Asset => "asset",
            Self::DamageScenario => "damage scenario",
            Self::ThreatScenario => "threat scenario",
            Self::AttackTree => "attack tree",
            Self::TreeNode => "tree node",
            Self::SecurityControl => "security control",
            Self::CybersecurityGoal => "cybersecurity goal",
            Self::CybersecurityClaim => "cybersecurity claim",
            Self::Element => "element",
        }
    }
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Errors returned by the model.
///
/// Every operation that returns a `ModelError` leaves the model unchanged.
///
/// ```
/// use aegis_model::{ModelError, NodeId};
///
/// assert_eq!("not a uuid".parse::<NodeId>(), Err(ModelError::InvalidUuid));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ModelError {
    /// The UUID is well-formed but is not an RFC 9562 version 7 UUID.
    #[error("UUID is not version 7")]
    NotVersion7,
    /// The text is not a valid UUID.
    #[error("text is not a valid UUID")]
    InvalidUuid,
    /// A node with this public ID already exists in the tree.
    #[error("node ID {0} already exists in the tree")]
    DuplicateId(NodeId),
    /// The index does not refer to a node in this tree.
    #[error("node index {} is not in the tree", .0.index())]
    UnknownNode(NodeIdx),
    /// The node is a step, so it cannot have children.
    #[error("node index {} is a step and cannot have children", .0.index())]
    NotAGate(NodeIdx),
    /// A new node was passed with children; new nodes must start empty.
    #[error("new node must not have children")]
    ChildrenNotEmpty,
    /// The tree cannot hold more nodes than a `u32` index can address.
    #[error("tree node capacity exceeded")]
    CapacityExceeded,
    /// The gate already has this node as a child; a gate holds each child at
    /// most once.
    ///
    /// Carries stable public IDs, which stay valid across reloads and in
    /// logs. Errors never contain labels (customer data).
    #[error("node {child} is already a child of node {parent}")]
    DuplicateEdge {
        /// The gate that already has the edge.
        parent: NodeId,
        /// The child that is already linked.
        child: NodeId,
    },
    /// [`AttackTree::link`](crate::AttackTree::link) refused an edge because
    /// `parent` is reachable from `child`, so the new edge would close a
    /// cycle.
    ///
    /// `path` is the existing chain from `child` down to `parent` in edge
    /// order (`path[0] == child`, last element `== parent`); the new edge
    /// `parent -> child` would close it. A self-link gives `path == [child]`.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let root_id = NodeId::new_v7();
    /// let g_id = NodeId::new_v7();
    /// let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "root"))?;
    /// let g = tree.add_child(tree.root(), g_id, Node::gate(GateKind::And, "g"))?;
    /// let err = tree.link(g, tree.root()).err();
    /// assert_eq!(
    ///     err,
    ///     Some(ModelError::WouldCreateCycle {
    ///         parent: g_id,
    ///         child: root_id,
    ///         path: vec![root_id, g_id],
    ///     }),
    /// );
    /// let text = err.map(|e| e.to_string()).unwrap_or_default();
    /// assert_eq!(
    ///     text,
    ///     format!(
    ///         "linking node {root_id} under node {g_id} would create a cycle: \
    ///          {g_id} -> {root_id} -> {g_id}"
    ///     ),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error(
        "linking node {child} under node {parent} would create a cycle: {parent} -> {}",
        path_text(path, false)
    )]
    WouldCreateCycle {
        /// The gate that was to receive the new child.
        parent: NodeId,
        /// The node that was to be linked.
        child: NodeId,
        /// Existing path from `child` down to `parent`, both included.
        path: Vec<NodeId>,
    },
    /// The graph contains a cycle, so it has no topological order.
    ///
    /// [`AttackTree::link`](crate::AttackTree::link) rejects cycles, so this
    /// is a defense-in-depth check before evaluation and is not expected
    /// from a tree built through the public API.
    ///
    /// The path lists the public IDs of one cycle in edge order: `path[i]`
    /// has `path[i + 1]` as a child, and the last node has `path[0]` as a
    /// child. It is non-empty, has no repeats, and starts with the node of
    /// smallest internal index, so the same tree always reports the same
    /// path. Display repeats the first ID at the end.
    ///
    /// ```
    /// use aegis_model::{ModelError, NodeId};
    ///
    /// let (a, b) = (NodeId::new_v7(), NodeId::new_v7());
    /// assert_eq!(
    ///     ModelError::Cycle(vec![a, b]).to_string(),
    ///     format!("cycle in attack tree: {a} -> {b} -> {a}"),
    /// );
    /// ```
    #[error("cycle in attack tree: {}", path_text(.0, true))]
    Cycle(Vec<NodeId>),
    /// [`evaluate`](crate::evaluate) found a gate with no children. Such a
    /// gate has no value: the domains have no neutral element, because
    /// treating an empty AND as "trivially feasible" would be the dangerous
    /// answer in a security tool. Carries the gate's public ID, never its
    /// label.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, GateKind, MinTime, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let id = NodeId::new_v7();
    /// let tree = AttackTree::new(id, Node::gate(GateKind::And, "empty"))?;
    /// let err = evaluate(&tree, &MinTime, |_| Some(1)).err();
    /// assert_eq!(err, Some(ModelError::EmptyGate(id)));
    /// assert_eq!(
    ///     err.map(|e| e.to_string()),
    ///     Some(format!("gate node {id} has no children, so it has no value")),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error("gate node {0} has no children, so it has no value")]
    EmptyGate(NodeId),
    /// The leaf-value callback of [`evaluate`](crate::evaluate) gave no
    /// value for this step. Evaluation never substitutes a default. Carries
    /// the step's public ID, never its label.
    ///
    /// ```
    /// use aegis_model::{evaluate, AttackTree, MinTime, ModelError, Node, NodeId};
    ///
    /// # fn main() -> Result<(), ModelError> {
    /// let id = NodeId::new_v7();
    /// let tree = AttackTree::new(id, Node::step("unrated"))?;
    /// let err = evaluate(&tree, &MinTime, |_| None).err();
    /// assert_eq!(err, Some(ModelError::MissingLeafValue(id)));
    /// assert_eq!(
    ///     err.map(|e| e.to_string()),
    ///     Some(format!("no value given for step node {id}")),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[error("no value given for step node {0}")]
    MissingLeafValue(NodeId),

    // -- Project ops ----------------------------------------------------------
    //
    // These carry public IDs, vocabulary indices, and counts, never entity
    // names (customer data).
    /// An asset with this ID already exists in the project.
    #[error("asset {0} already exists in the project")]
    DuplicateAsset(AssetId),
    /// A damage scenario with this ID already exists in the project.
    #[error("damage scenario {0} already exists in the project")]
    DuplicateDamageScenario(DamageId),
    /// A threat scenario with this ID already exists in the project.
    #[error("threat scenario {0} already exists in the project")]
    DuplicateThreatScenario(ThreatId),
    /// An attack tree with this ID already exists in the project.
    #[error("attack tree {0} already exists in the project")]
    DuplicateAttackTree(TreeId),
    /// A security control with this ID already exists in the project.
    #[error("security control {0} already exists in the project")]
    DuplicateSecurityControl(ControlId),
    /// A cybersecurity goal with this ID already exists in the project.
    #[error("cybersecurity goal {0} already exists in the project")]
    DuplicateCybersecurityGoal(GoalId),
    /// A cybersecurity claim with this ID already exists in the project.
    #[error("cybersecurity claim {0} already exists in the project")]
    DuplicateCybersecurityClaim(ClaimId),
    /// An element with this ID already exists in the project.
    #[error("element {0} already exists in the project")]
    DuplicateElement(ElementId),
    /// An element attribute key is not in pack key syntax.
    #[error("an attribute key of element {element} is not valid")]
    InvalidAttributeKey {
        /// The element.
        element: ElementId,
    },
    /// An element attribute value is not valid for its type.
    #[error("an attribute value of element {element} is not valid")]
    InvalidAttributeValue {
        /// The element.
        element: ElementId,
    },
    /// The element already has the most attributes.
    #[error("element {element} already has {max} attributes")]
    TooManyAttributes {
        /// The element.
        element: ElementId,
        /// The cap.
        max: usize,
    },
    /// The element has attributes and cannot be removed.
    #[error("element {element} still has {count} attributes; clear them first")]
    ElementHasAttributes {
        /// The element.
        element: ElementId,
        /// How many are set.
        count: usize,
    },
    /// A component with this ID already exists in the project.
    #[error("component {0} already exists in the project")]
    DuplicateComponent(ComponentId),
    /// An interface with this ID already exists in the project.
    #[error("interface {0} already exists in the project")]
    DuplicateInterface(InterfaceId),
    /// A data flow with this ID already exists in the project.
    #[error("data flow {0} already exists in the project")]
    DuplicateDataFlow(DataFlowId),
    /// A trust boundary with this ID already exists in the project.
    #[error("trust boundary {0} already exists in the project")]
    DuplicateTrustBoundary(TrustBoundaryId),
    /// No asset with this ID is in the project.
    #[error("asset {0} is not in the project")]
    UnknownAsset(AssetId),
    /// No damage scenario with this ID is in the project.
    #[error("damage scenario {0} is not in the project")]
    UnknownDamageScenario(DamageId),
    /// No threat scenario with this ID is in the project.
    #[error("threat scenario {0} is not in the project")]
    UnknownThreatScenario(ThreatId),
    /// No attack tree with this ID is in the project.
    #[error("attack tree {0} is not in the project")]
    UnknownAttackTree(TreeId),
    /// No security control with this ID is in the project.
    #[error("security control {0} is not in the project")]
    UnknownSecurityControl(ControlId),
    /// No cybersecurity goal with this ID is in the project.
    #[error("cybersecurity goal {0} is not in the project")]
    UnknownCybersecurityGoal(GoalId),
    /// No cybersecurity claim with this ID is in the project.
    #[error("cybersecurity claim {0} is not in the project")]
    UnknownCybersecurityClaim(ClaimId),
    /// No element with this ID is in the project.
    #[error("element {0} is not in the project")]
    UnknownElement(ElementId),
    /// No component with this ID is in the project.
    #[error("component {0} is not in the project")]
    UnknownComponent(ComponentId),
    /// No interface with this ID is in the project.
    #[error("interface {0} is not in the project")]
    UnknownInterface(InterfaceId),
    /// No data flow with this ID is in the project.
    #[error("data flow {0} is not in the project")]
    UnknownDataFlow(DataFlowId),
    /// No trust boundary with this ID is in the project.
    #[error("trust boundary {0} is not in the project")]
    UnknownTrustBoundary(TrustBoundaryId),
    /// No node with this ID is in the attack tree.
    #[error("node {node} is not in attack tree {tree}")]
    UnknownTreeNode {
        /// The attack tree.
        tree: TreeId,
        /// The missing node ID.
        node: NodeId,
    },
    /// A gate node was given where a leaf step is required.
    #[error("node {node} of attack tree {tree} is a gate, not a step")]
    NotAStep {
        /// The attack tree.
        tree: TreeId,
        /// The gate node ID.
        node: NodeId,
    },
    /// A name is empty after trimming whitespace.
    #[error("{kind} name is empty")]
    EmptyName {
        /// Which entity kind had an empty name.
        kind: EntityKind,
    },
    /// A node is placed where it cannot go: an arena index that is not after
    /// its owner or not within the tree, or a position past the end of a
    /// child list or of the extra parents of a node.
    #[error("the node cannot be placed there")]
    BadNodePlacement,
    /// The root of an attack tree cannot be removed.
    #[error("the root of an attack tree cannot be removed")]
    CannotRemoveRoot,
    /// A node with children cannot be removed; remove the children first.
    #[error("the node {0} has children")]
    NodeHasChildren(NodeId),
    /// A node that is also a child of another gate cannot be removed;
    /// unlink it first.
    #[error("the node {0} is shared with another gate")]
    NodeIsShared(NodeId),
    /// A step with a rating, factors or mitigations cannot be removed; clear
    /// them first.
    #[error("the step {0} still has a rating, factors or controls")]
    NodeIsRated(NodeId),
    /// `child` is not an extra child of `parent` (there is no such edge, or
    /// it is the owner's edge, which unlinking does not remove).
    #[error("{child} is not linked under {parent}")]
    NotLinked {
        /// The gate.
        parent: NodeId,
        /// The node.
        child: NodeId,
    },
    /// The attack tree has too many nodes to be added or removed by one op;
    /// take it apart node by node.
    #[error("attack tree {tree} has more than {max} nodes for one op")]
    TreeTooLargeForOneOp {
        /// The tree.
        tree: TreeId,
        /// The limit, in nodes.
        max: usize,
    },
    /// A description or notes text is longer than `max` chars.
    #[error("text is longer than {max} chars")]
    DescriptionTooLong {
        /// The limit, in chars.
        max: usize,
    },
    /// A description or notes text has a control character other than a tab
    /// or a line break. Carries no text.
    #[error("text contains a control character")]
    InvalidText,
    /// All descriptions of the project together would exceed `max` bytes.
    #[error("the descriptions of the project are larger than {max} bytes together")]
    DescriptionTotalTooLarge {
        /// The limit, in bytes.
        max: usize,
    },
    /// A {kind} with a description cannot be removed; clear the description
    /// first (in the same batch).
    #[error("the {kind} has a description; clear it before removing the {kind}")]
    DescriptionNotCleared {
        /// Which kind of element.
        kind: EntityKind,
    },
    /// A translation was set on a target that has no description text.
    #[error("the element has no description to translate")]
    NoDescription,
    /// A description has more translations than `max`.
    #[error("a description has more than {max} translations")]
    TooManyTranslations {
        /// The limit.
        max: usize,
    },
    /// The project lists more than `max` languages.
    #[error("the project lists more than {max} languages")]
    TooManyLanguages {
        /// The limit.
        max: usize,
    },
    /// The project lists one language twice.
    #[error("the project lists a language twice")]
    DuplicateLanguage,
    /// A name is longer than `max` chars.
    #[error("{kind} name is longer than {max} chars")]
    NameTooLong {
        /// Which entity kind had a name that was too long.
        kind: EntityKind,
        /// The limit, in chars.
        max: usize,
    },
    /// A non-empty provenance actor string contains only whitespace.
    ///
    /// Carries no actor text so user or customer identifiers never appear in
    /// error messages or logs.
    ///
    /// ```
    /// use aegis_model::ModelError;
    ///
    /// assert_eq!(
    ///     ModelError::EmptyActor.to_string(),
    ///     "provenance actor is empty after trimming whitespace",
    /// );
    /// ```
    #[error("provenance actor is empty after trimming whitespace")]
    EmptyActor,
    /// A provenance actor string is longer than `max` chars.
    ///
    /// Carries only the character limit, never the actor text.
    ///
    /// ```
    /// use aegis_model::{ModelError, MAX_NAME_CHARS};
    ///
    /// let err = ModelError::ActorTooLong { max: MAX_NAME_CHARS };
    /// assert_eq!(
    ///     err.to_string(),
    ///     "provenance actor is longer than 256 chars",
    /// );
    /// ```
    #[error("provenance actor is longer than {max} chars")]
    ActorTooLong {
        /// The limit, in chars.
        max: usize,
    },
    /// The impact category index is not in the project's methodology.
    #[error("impact category index {} is not in the methodology", .0.index())]
    UnknownImpactCategory(ImpactCategory),
    /// The level index is not in that impact category's scale.
    #[error(
        "level index {} is not in the scale of impact category index {}",
        .level.index(),
        .category.index()
    )]
    UnknownLevel {
        /// The impact category.
        category: ImpactCategory,
        /// The out-of-range level.
        level: Level,
    },
    /// The feasibility level index is not in the project's methodology.
    #[error("feasibility level index {} is not in the methodology", .0.index())]
    UnknownFeasibilityLevel(FeasibilityLevel),
    /// The attack potential factor index is not in the project's methodology.
    #[error("feasibility factor index {} is not in the methodology", .0.index())]
    UnknownFeasibilityFactor(FeasibilityFactor),
    /// The factor level index is not in that attack potential factor's scale.
    #[error(
        "factor level index {} is not in the scale of feasibility factor index {}",
        .level.index(),
        .factor.index()
    )]
    UnknownFactorLevel {
        /// The attack potential factor.
        factor: FeasibilityFactor,
        /// The out-of-range factor level.
        level: FactorLevel,
    },
    /// The risk level index is not in the project's methodology.
    #[error("risk level index {} is not in the methodology", .0.index())]
    UnknownRiskLevel(RiskLevel),
    /// The property index is not in the project's methodology.
    #[error("property index {} is not in the methodology", .0.index())]
    UnknownProperty(Property),
    /// The threat category index is not in the project's methodology.
    #[error("threat category index {} is not in the methodology", .0.index())]
    UnknownThreatCategory(ThreatCategory),
    /// The catalog scheme index is not in the project's methodology.
    #[error("catalog scheme index {} is not in the methodology", .0.index())]
    UnknownCatalogScheme(CatalogScheme),
    /// The interface kind index is not in the project's methodology.
    #[error("interface kind index {} is not in the methodology", .0.index())]
    UnknownInterfaceKind(InterfaceKind),
    /// The attack type index is not in the project's methodology.
    #[error("attack type index {} is not in the methodology", .0.index())]
    UnknownAttackType(AttackType),
    /// Setting `parent` on `component` would create a cycle in the component hierarchy.
    #[error("setting component {parent} as parent of {component} would create a cycle")]
    ComponentCycle {
        /// The component being reparented.
        component: ComponentId,
        /// The proposed parent component.
        parent: ComponentId,
    },
    /// Setting `parent` on `component` would exceed the maximum component nesting depth.
    #[error("component {component} would exceed maximum hierarchy depth {max}")]
    ComponentDepthExceeded {
        /// The component exceeding the limit.
        component: ComponentId,
        /// Maximum allowed depth.
        max: usize,
    },
    /// The component cannot be removed while child components still reference it.
    #[error("component {component} has {children} child component(s)")]
    ComponentHasChildren {
        /// The component.
        component: ComponentId,
        /// How many child components reference it.
        children: u32,
    },
    /// The component cannot be removed while interfaces still belong to it.
    #[error("component {component} has {interfaces} interface(s)")]
    ComponentHasInterfaces {
        /// The component.
        component: ComponentId,
        /// How many interfaces belong to it.
        interfaces: u32,
    },
    /// The component cannot be removed while data flows still reference it.
    #[error("component {component} is referenced by {flows} data flow(s)")]
    ComponentInDataFlow {
        /// The component.
        component: ComponentId,
        /// How many data flows reference it.
        flows: u32,
    },
    /// The component cannot be removed while trust boundaries still contain it.
    #[error("component {component} is in {boundaries} trust boundary(ies)")]
    ComponentInTrustBoundary {
        /// The component.
        component: ComponentId,
        /// How many trust boundaries contain it.
        boundaries: u32,
    },
    /// The component cannot be removed while attack steps still go through it.
    #[error("component {component} has {steps} step(s) going through it")]
    ComponentHasSteps {
        /// The component.
        component: ComponentId,
        /// How many steps go through it.
        steps: u32,
    },
    /// The interface cannot be removed while attack steps still go through it.
    #[error("interface {interface} has {steps} step(s) going through it")]
    InterfaceHasSteps {
        /// The interface.
        interface: InterfaceId,
        /// How many steps go through it.
        steps: u32,
    },
    /// The component cannot be removed while security controls are still applied to it.
    #[error("component {component} has {controls} security control(s) applied")]
    ComponentHasControls {
        /// The component.
        component: ComponentId,
        /// How many security controls are applied to it.
        controls: u32,
    },
    /// The interface cannot be removed while security controls are still applied to it.
    #[error("interface {interface} has {controls} security control(s) applied")]
    InterfaceHasControls {
        /// The interface.
        interface: InterfaceId,
        /// How many security controls are applied to it.
        controls: u32,
    },
    /// The component cannot be removed while assets are still bound to it.
    #[error("component {component} is bound to {assets} asset(s)")]
    ComponentHasAssets {
        /// The component.
        component: ComponentId,
        /// How many assets are bound to it.
        assets: u32,
    },
    /// The asset cannot be removed while it is still bound to a component; unbind it first.
    #[error("asset {asset} is still bound to component {component}")]
    AssetHasComponent {
        /// The asset.
        asset: AssetId,
        /// The bound component.
        component: ComponentId,
    },
    /// The interface cannot be removed while data flows still reference it.
    #[error("interface {interface} is referenced by {flows} data flow(s)")]
    InterfaceInUse {
        /// The interface.
        interface: InterfaceId,
        /// How many data flows reference it.
        flows: u32,
    },
    /// The interface of a data flow does not belong to either its source or destination component.
    #[error(
        "interface {interface} of data flow {flow} does not belong to its source or destination component"
    )]
    InterfaceNotOnEndpoint {
        /// The data flow.
        flow: DataFlowId,
        /// The interface.
        interface: InterfaceId,
    },
    /// The trust boundary already contains this component, or an op lists it twice.
    #[error("trust boundary {boundary} already contains component {component}")]
    DuplicateBoundaryComponent {
        /// The trust boundary.
        boundary: TrustBoundaryId,
        /// The component.
        component: ComponentId,
    },
    /// The trust boundary does not contain this component.
    #[error("trust boundary {boundary} does not contain component {component}")]
    BoundaryComponentNotLinked {
        /// The trust boundary.
        boundary: TrustBoundaryId,
        /// The component.
        component: ComponentId,
    },
    /// A trust boundary may contain at most `max` components.
    #[error("trust boundary {boundary} would contain more than {max} components")]
    TooManyBoundaryMembers {
        /// The trust boundary.
        boundary: TrustBoundaryId,
        /// The limit.
        max: usize,
    },
    /// The catalog ID does not match its scheme's syntax.
    #[error(
        "catalog ID does not match syntax {syntax} of scheme index {}",
        .scheme.index()
    )]
    InvalidCatalogId {
        /// The scheme whose syntax was not met.
        scheme: CatalogScheme,
        /// The expected syntax of that scheme.
        syntax: CatalogSyntax,
    },
    /// An asset must have at least one security property.
    #[error("asset {0} has no security properties")]
    NoSecurityProperties(AssetId),
    /// The threat category violates a property the asset does not carry.
    #[error("asset {asset} does not carry property index {}", .property.index())]
    PropertyNotOnAsset {
        /// The targeted asset.
        asset: AssetId,
        /// The missing property.
        property: Property,
    },
    /// The property cannot be dropped: `threats` threats target it on this
    /// asset.
    #[error(
        "property index {} of asset {asset} is targeted by {threats} threat(s)",
        .property.index()
    )]
    PropertyInUse {
        /// The asset.
        asset: AssetId,
        /// The property still in use.
        property: Property,
        /// How many threats target it.
        threats: u32,
    },
    /// The asset cannot be removed: `threats` threats target it.
    #[error("asset {asset} is targeted by {threats} threat(s)")]
    AssetInUse {
        /// The asset.
        asset: AssetId,
        /// How many threats target it.
        threats: u32,
    },
    /// The damage scenario cannot be removed: `threats` threats link to it.
    #[error("damage scenario {damage} is linked by {threats} threat(s)")]
    DamageScenarioInUse {
        /// The damage scenario.
        damage: DamageId,
        /// How many threats link to it.
        threats: u32,
    },
    /// The attack tree cannot be removed: `threats` threats reference it.
    #[error("attack tree {tree} is referenced by {threats} threat(s)")]
    AttackTreeInUse {
        /// The attack tree.
        tree: TreeId,
        /// How many threats reference it.
        threats: u32,
    },
    /// The attack tree cannot be removed while steps in it still have a
    /// "through" item or an attack type; clear them first.
    #[error("attack tree {tree} still has {steps} step(s) with a through item or attack type")]
    AttackTreeHasStepLinks {
        /// The attack tree.
        tree: TreeId,
        /// How many steps have one.
        steps: usize,
    },
    /// The attack tree cannot be removed while it still has security control
    /// mitigations attached to its steps; clear the mitigations first.
    #[error("attack tree {tree} still has {mitigations} step mitigation(s)")]
    AttackTreeHasMitigations {
        /// The attack tree.
        tree: TreeId,
        /// How many step mitigations are attached across the tree.
        mitigations: usize,
    },
    /// The attack tree cannot be removed while it still has attack potential
    /// factor ratings attached to its steps; clear the factor ratings first.
    #[error("attack tree {tree} still has {factors} step factor rating(s)")]
    AttackTreeHasFactors {
        /// The attack tree.
        tree: TreeId,
        /// How many step factor ratings are attached across the tree.
        factors: usize,
    },
    /// The security control cannot be removed: `steps` step mitigations
    /// reference it.
    #[error("security control {control} is attached to {steps} step(s)")]
    SecurityControlInUse {
        /// The security control.
        control: ControlId,
        /// How many attack tree steps reference it.
        steps: u32,
    },
    /// The security control cannot be removed: `goals` cybersecurity goals
    /// link to it.
    #[error("security control {control} is linked by {goals} cybersecurity goal(s)")]
    SecurityControlInGoal {
        /// The security control.
        control: ControlId,
        /// How many cybersecurity goals link to it.
        goals: u32,
    },
    /// The security control cannot be removed while it is still applied to
    /// components or interfaces; unapply it first.
    #[error("security control {control} is applied to {targets} target(s)")]
    SecurityControlHasTargets {
        /// The security control.
        control: ControlId,
        /// How many components and interfaces it is applied to.
        targets: usize,
    },
    /// The security control cannot be removed while it still carries control
    /// rules; clear the rules first.
    #[error("security control {control} still has {rules} control rule(s)")]
    SecurityControlHasRules {
        /// The security control.
        control: ControlId,
        /// How many control rules it carries.
        rules: usize,
    },
    /// The security control is already applied to this target, or a snapshot
    /// lists it twice.
    #[error("security control {control} is already applied to {target}")]
    DuplicateControlTarget {
        /// The security control.
        control: ControlId,
        /// The component or interface.
        target: ItemRef,
    },
    /// The security control is not applied to this target.
    #[error("security control {control} is not applied to {target}")]
    ControlTargetNotLinked {
        /// The security control.
        control: ControlId,
        /// The component or interface.
        target: ItemRef,
    },
    /// A security control may be applied to at most `max` targets.
    ///
    /// ```
    /// use aegis_model::{ControlId, ModelError, MAX_CONTROL_TARGETS};
    ///
    /// let control = ControlId::new_v7();
    /// let err = ModelError::TooManyControlTargets { control, max: MAX_CONTROL_TARGETS };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!("security control {control} would be applied to more than 64 targets"),
    /// );
    /// ```
    #[error("security control {control} would be applied to more than {max} targets")]
    TooManyControlTargets {
        /// The security control.
        control: ControlId,
        /// The limit.
        max: usize,
    },
    /// A security control may carry at most `max` control rules.
    ///
    /// ```
    /// use aegis_model::{ControlId, ModelError, MAX_CONTROL_RULES};
    ///
    /// let control = ControlId::new_v7();
    /// let err = ModelError::TooManyControlRules { control, max: MAX_CONTROL_RULES };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!("security control {control} would have more than 16 control rules"),
    /// );
    /// ```
    #[error("security control {control} would have more than {max} control rules")]
    TooManyControlRules {
        /// The security control.
        control: ControlId,
        /// The limit.
        max: usize,
    },
    /// The cybersecurity goal cannot be removed: `threats` threat scenarios
    /// reference it.
    #[error("cybersecurity goal {goal} is referenced by {threats} threat(s)")]
    CybersecurityGoalInUse {
        /// The cybersecurity goal.
        goal: GoalId,
        /// How many threat scenarios reference it.
        threats: u32,
    },
    /// The cybersecurity claim cannot be removed: `threats` threat scenarios
    /// reference it.
    #[error("cybersecurity claim {claim} is referenced by {threats} threat(s)")]
    CybersecurityClaimInUse {
        /// The cybersecurity claim.
        claim: ClaimId,
        /// How many threat scenarios reference it.
        threats: u32,
    },
    /// The threat scenario cannot be removed while it still references an
    /// attack tree; unlink the tree first.
    #[error("threat scenario {threat} still references attack tree {tree}")]
    ThreatHasAttackTree {
        /// The threat scenario.
        threat: ThreatId,
        /// The linked attack tree.
        tree: TreeId,
    },
    /// The threat scenario cannot be removed while it still has a direct
    /// feasibility rating; clear the rating first.
    #[error(
        "threat scenario {threat} still has direct feasibility level index {}",
        feasibility.index()
    )]
    ThreatHasDirectFeasibility {
        /// The threat scenario.
        threat: ThreatId,
        /// The direct feasibility level on the threat scenario.
        feasibility: FeasibilityLevel,
    },
    /// The threat scenario cannot be removed while it still has a risk
    /// treatment decision; clear the treatment decision first.
    #[error("threat scenario {threat} still has risk treatment {treatment}")]
    ThreatHasRiskTreatment {
        /// The threat scenario.
        threat: ThreatId,
        /// The risk treatment decision on the threat scenario.
        treatment: RiskTreatment,
    },
    /// The threat scenario cannot be removed while it still references a
    /// cybersecurity goal; unlink the goal first.
    #[error("threat scenario {threat} still references cybersecurity goal {goal}")]
    ThreatHasGoal {
        /// The threat scenario.
        threat: ThreatId,
        /// The linked cybersecurity goal.
        goal: GoalId,
    },
    /// The threat scenario cannot be removed while it still references a
    /// cybersecurity claim; unlink the claim first.
    #[error("threat scenario {threat} still references cybersecurity claim {claim}")]
    ThreatHasClaim {
        /// The threat scenario.
        threat: ThreatId,
        /// The linked cybersecurity claim.
        claim: ClaimId,
    },
    /// The threat scenario cannot be removed while it still has a scaling
    /// rating or reasoning (FND-10); clear it first, so the removal's
    /// inverse stays exact.
    #[error("threat scenario {threat} still has a scaling rating")]
    ThreatHasScaling {
        /// The threat scenario.
        threat: ThreatId,
    },
    /// The threat scenario cannot be removed while it still has a
    /// feasibility rationale (FND-09); clear it first.
    #[error("threat scenario {threat} still has a feasibility rationale")]
    ThreatHasRationale {
        /// The threat scenario.
        threat: ThreatId,
    },
    /// The damage scenario cannot be removed while it still has an impact
    /// rationale (FND-09); clear it first.
    #[error("damage scenario {damage} still has an impact rationale")]
    DamageHasRationale {
        /// The damage scenario.
        damage: DamageId,
    },
    /// The threat scenario has no linked attack tree to evaluate.
    #[error("threat scenario {0} has no linked attack tree")]
    NoAttackTree(ThreatId),
    /// An impacts list names the same category twice.
    #[error("impact category index {} is listed twice", .0.index())]
    DuplicateImpactCategory(ImpactCategory),
    /// A step feasibility list rates the same step node twice.
    #[error("step node {node} of attack tree {tree} is rated twice")]
    DuplicateStepFeasibility {
        /// The attack tree.
        tree: TreeId,
        /// The repeated step node ID.
        node: NodeId,
    },
    /// The threat already links to this damage scenario, or an op lists it
    /// twice.
    #[error("threat scenario {threat} already links damage scenario {damage}")]
    DuplicateDamageLink {
        /// The threat scenario.
        threat: ThreatId,
        /// The damage scenario.
        damage: DamageId,
    },
    /// The threat does not link to this damage scenario.
    #[error("threat scenario {threat} does not link damage scenario {damage}")]
    DamageNotLinked {
        /// The threat scenario.
        threat: ThreatId,
        /// The damage scenario.
        damage: DamageId,
    },
    /// The cybersecurity goal already links to this security control, or an op
    /// lists it twice.
    #[error("cybersecurity goal {goal} already links security control {control}")]
    DuplicateGoalControl {
        /// The cybersecurity goal.
        goal: GoalId,
        /// The security control.
        control: ControlId,
    },
    /// The cybersecurity goal does not link to this security control.
    #[error("cybersecurity goal {goal} does not link security control {control}")]
    GoalControlNotLinked {
        /// The cybersecurity goal.
        goal: GoalId,
        /// The security control.
        control: ControlId,
    },
    /// A cybersecurity goal may link to at most `max` security controls.
    ///
    /// ```
    /// use aegis_model::{GoalId, ModelError, MAX_GOAL_CONTROLS};
    ///
    /// let goal = GoalId::new_v7();
    /// let err = ModelError::TooManyGoalControls { goal, max: MAX_GOAL_CONTROLS };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!("cybersecurity goal {goal} would link more than 16 security controls"),
    /// );
    /// ```
    #[error("cybersecurity goal {goal} would link more than {max} security controls")]
    TooManyGoalControls {
        /// The cybersecurity goal.
        goal: GoalId,
        /// The limit.
        max: usize,
    },
    /// The threat does not have this catalog ref.
    #[error("threat scenario {threat} does not have catalog ref {catalog_ref}")]
    CatalogRefNotFound {
        /// The threat scenario.
        threat: ThreatId,
        /// The catalog ref that was not on the threat scenario.
        catalog_ref: CatalogRef,
    },
    /// The threat already has this catalog ref, or an op lists it twice.
    #[error("threat scenario {threat} already has catalog ref {catalog_ref}")]
    DuplicateCatalogRef {
        /// The threat scenario.
        threat: ThreatId,
        /// The duplicated catalog ref.
        catalog_ref: CatalogRef,
    },
    /// A threat may have at most `max` catalog refs.
    #[error("threat scenario {threat} would have more than {max} catalog refs")]
    TooManyCatalogRefs {
        /// The threat scenario.
        threat: ThreatId,
        /// The limit.
        max: usize,
    },
    /// A threat may link to at most `max` damage scenarios.
    ///
    /// ```
    /// use aegis_model::{ModelError, ThreatId, MAX_DAMAGE_LINKS};
    ///
    /// let threat = ThreatId::new_v7();
    /// let err = ModelError::TooManyDamageLinks { threat, max: MAX_DAMAGE_LINKS };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!("threat scenario {threat} would link more than 256 damage scenarios"),
    /// );
    /// ```
    #[error("threat scenario {threat} would link more than {max} damage scenarios")]
    TooManyDamageLinks {
        /// The threat scenario.
        threat: ThreatId,
        /// The limit.
        max: usize,
    },
    /// An attack tree in a project may have at most `max` nodes.
    ///
    /// ```
    /// use aegis_model::{ModelError, TreeId, MAX_ATTACK_TREE_NODES};
    ///
    /// let tree = TreeId::new_v7();
    /// let err = ModelError::TooManyAttackTreeNodes { tree, max: MAX_ATTACK_TREE_NODES };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!("attack tree {tree} would have more than {MAX_ATTACK_TREE_NODES} nodes"),
    /// );
    /// ```
    #[error("attack tree {tree} would have more than {max} nodes")]
    TooManyAttackTreeNodes {
        /// The attack tree.
        tree: TreeId,
        /// The limit.
        max: usize,
    },
    /// An attack step may have at most `max` security control mitigations.
    ///
    /// ```
    /// use aegis_model::{ModelError, NodeId, TreeId, MAX_STEP_MITIGATIONS};
    ///
    /// let tree = TreeId::new_v7();
    /// let node = NodeId::new_v7();
    /// let err = ModelError::TooManyStepMitigations { tree, node, max: MAX_STEP_MITIGATIONS };
    /// assert_eq!(
    ///     err.to_string(),
    ///     format!(
    ///         "step node {node} of attack tree {tree} would have more than 16 security control mitigations"
    ///     ),
    /// );
    /// ```
    #[error(
        "step node {node} of attack tree {tree} would have more than {max} security control mitigations"
    )]
    TooManyStepMitigations {
        /// The attack tree.
        tree: TreeId,
        /// The step node.
        node: NodeId,
        /// The limit.
        max: usize,
    },
    /// The project cannot hold more entities of this kind (the arena is at
    /// its slot limit, at most `u32::MAX`).
    #[error("project {kind} capacity exceeded")]
    ProjectCapacityExceeded {
        /// Which entity kind hit its slot limit.
        kind: EntityKind,
    },
    /// Two projects cannot be compared by [`Project::diff`](crate::Project::diff)
    /// because they use different methodologies.
    ///
    /// ```
    /// use aegis_model::ModelError;
    ///
    /// let err = ModelError::MethodologyMismatch {
    ///     expected: "m1".into(),
    ///     expected_version: "1".into(),
    ///     found: "m2".into(),
    ///     found_version: "1".into(),
    /// };
    /// assert_eq!(
    ///     err.to_string(),
    ///     "projects use different methodologies: expected m1 v1, found m2 v1"
    /// );
    /// ```
    #[error(
        "projects use different methodologies: expected {expected} v{expected_version}, found {found} v{found_version}"
    )]
    MethodologyMismatch {
        /// Baseline methodology key.
        expected: Arc<str>,
        /// Baseline methodology version.
        expected_version: Arc<str>,
        /// Target methodology key.
        found: Arc<str>,
        /// Target methodology version.
        found_version: Arc<str>,
    },
}

/// Errors from [`Methodology::new`](crate::Methodology::new): one variant
/// per rule a [`MethodologyDef`](crate::MethodologyDef) must meet.
///
/// Variants carry vocabulary keys (methodology data, not project data).
/// Every key they hold is at most 64 bytes: keys that passed the key check
/// are at most 64 bytes by rule, and the variants that can hold a key
/// that did not ([`InvalidKey`](Self::InvalidKey),
/// [`UnknownRiskLevel`](Self::UnknownRiskLevel), and
/// [`UnknownViolatedProperty`](Self::UnknownViolatedProperty)) keep at
/// most 64 bytes of it, so an error built from hostile input stays small.
///
/// ```
/// use aegis_model::{ElementNamesDef, Methodology, MethodologyDef, MethodologyError};
///
/// let def = MethodologyDef {
///     key: "empty".into(),
///     version: "1".into(),
///     label: "Empty".into(),
///     element_names: ElementNamesDef::default(),
///     impact_categories: vec![],
///     feasibility_levels: vec![],
///     risk_levels: vec![],
///     properties: vec![],
///     threat_categories: vec![],
///     catalog_schemes: vec![],
///     require_rationale: false,
///     attack_potential: None,
/// };
/// assert_eq!(Methodology::new(def), Err(MethodologyError::NoImpactCategory));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MethodologyError {
    /// A key does not match `[a-z][a-z0-9_-]{0,63}`.
    ///
    /// Holds at most 64 bytes of the key: a key of 64 bytes or fewer is
    /// kept exactly; a longer one is cut at a char boundary to at most 61
    /// bytes and ends in `...` (`.` is not a key char, so a cut text never
    /// looks like a valid key).
    ///
    /// ```
    /// use aegis_model::{ElementNamesDef, Methodology, MethodologyDef, MethodologyError};
    ///
    /// let def = MethodologyDef {
    ///     key: "K".repeat(100_000).into(),
    ///     version: "1".into(),
    ///     label: "Huge".into(),
    ///     element_names: ElementNamesDef::default(),
    ///     impact_categories: vec![],
    ///     feasibility_levels: vec![],
    ///     risk_levels: vec![],
    ///     properties: vec![],
    ///     threat_categories: vec![],
    ///     catalog_schemes: vec![],
    ///     require_rationale: false,
    ///     attack_potential: None,
    /// };
    /// let kept = format!("{}...", "K".repeat(61));
    /// assert_eq!(Methodology::new(def), Err(MethodologyError::InvalidKey(kept.into())));
    /// ```
    #[error("key {0:?} does not match [a-z][a-z0-9_-]{{0,63}}")]
    InvalidKey(Arc<str>),
    /// The version is blank or longer than 64 chars.
    #[error("version must be 1 to 64 chars and not blank")]
    InvalidVersion,
    /// The label of the item with this key is blank.
    #[error("label of {key:?} is blank")]
    BlankLabel {
        /// Key of the item.
        key: Arc<str>,
    },
    /// The label of the item with this key is longer than `max` chars.
    #[error("label of {key:?} is longer than {max} chars")]
    LabelTooLong {
        /// Key of the item.
        key: Arc<str>,
        /// The limit, in chars.
        max: usize,
    },
    /// No impact category is declared.
    #[error("at least one impact category is required")]
    NoImpactCategory,
    /// More than `max` impact categories are declared.
    #[error("more than {max} impact categories")]
    TooManyImpactCategories {
        /// The limit.
        max: usize,
    },
    /// A rating scale has fewer than `min` or more than `max` levels.
    #[error("impact category {category:?} must have {min} to {max} levels")]
    LevelCount {
        /// Key of the impact category.
        category: Arc<str>,
        /// Fewest levels.
        min: usize,
        /// Most levels.
        max: usize,
    },
    /// Two impact categories share a key.
    #[error("impact category key {0:?} is declared twice")]
    DuplicateImpactCategory(Arc<str>),
    /// Two levels of one category share a key.
    #[error("level key {level:?} is declared twice in impact category {category:?}")]
    DuplicateLevel {
        /// Key of the impact category.
        category: Arc<str>,
        /// The repeated level key.
        level: Arc<str>,
    },
    /// `feasibility_levels` has fewer than `min` or more than `max` levels.
    #[error("feasibility_levels has fewer than {min} or more than {max} levels")]
    FeasibilityLevelCount {
        /// Fewest levels.
        min: usize,
        /// Most levels.
        max: usize,
    },
    /// Two feasibility levels share a key.
    #[error("duplicate feasibility level key {0:?}")]
    DuplicateFeasibilityLevel(Arc<str>),
    /// `risk_levels` has fewer than `min` or more than `max` levels.
    #[error("risk_levels has fewer than {min} or more than {max} levels")]
    RiskLevelCount {
        /// Fewest levels.
        min: usize,
        /// Most levels.
        max: usize,
    },
    /// Two risk levels share a key.
    #[error("duplicate risk level key {0:?}")]
    DuplicateRiskLevel(Arc<str>),
    /// An impact category's `risk_matrix` does not have one row per impact
    /// level and one column per feasibility level.
    #[error("impact category {category:?} risk_matrix must be {expected_rows}x{expected_cols}")]
    RiskMatrixDimensions {
        /// Key of the impact category.
        category: Arc<str>,
        /// Expected row count (`levels.len()`).
        expected_rows: usize,
        /// Expected column count (`feasibility_levels.len()`).
        expected_cols: usize,
    },
    /// An impact category's `risk_matrix` names a risk level that is not
    /// declared in `risk_levels`.
    #[error("impact category {category:?} risk_matrix names unknown risk level {risk:?}")]
    UnknownRiskLevel {
        /// Key of the impact category.
        category: Arc<str>,
        /// The unknown risk level key, at most 64 bytes of it (cut like
        /// [`InvalidKey`](Self::InvalidKey)'s).
        risk: Arc<str>,
    },
    /// No security property is declared.
    #[error("at least one security property is required")]
    NoProperty,
    /// More than `max` security properties are declared.
    #[error("more than {max} security properties")]
    TooManyProperties {
        /// The limit.
        max: usize,
    },
    /// Two properties share a key.
    #[error("property key {0:?} is declared twice")]
    DuplicateProperty(Arc<str>),
    /// No threat category is declared.
    #[error("at least one threat category is required")]
    NoThreatCategory,
    /// More than `max` threat categories are declared.
    #[error("more than {max} threat categories")]
    TooManyThreatCategories {
        /// The limit.
        max: usize,
    },
    /// Two threat categories share a key.
    #[error("threat category key {0:?} is declared twice")]
    DuplicateThreatCategory(Arc<str>),
    /// A threat category violates no property.
    #[error("threat category {category:?} violates no property")]
    EmptyViolates {
        /// Key of the threat category.
        category: Arc<str>,
    },
    /// A threat category names a property that is not declared.
    #[error("threat category {category:?} names unknown property {property:?}")]
    UnknownViolatedProperty {
        /// Key of the threat category.
        category: Arc<str>,
        /// The unknown property key, at most 64 bytes of it (cut like
        /// [`InvalidKey`](Self::InvalidKey)'s).
        property: Arc<str>,
    },
    /// A threat category names the same property twice.
    #[error("threat category {category:?} names property {property:?} twice")]
    RepeatedViolatedProperty {
        /// Key of the threat category.
        category: Arc<str>,
        /// The repeated property key.
        property: Arc<str>,
    },
    /// More than `max` catalog schemes are declared.
    #[error("more than {max} catalog schemes")]
    TooManyCatalogSchemes {
        /// The limit.
        max: usize,
    },
    /// Two catalog schemes share a key.
    #[error("catalog scheme key {0:?} is declared twice")]
    DuplicateCatalogScheme(Arc<str>),
    /// A catalog syntax has fewer than 1 or more than 4 segments.
    #[error("catalog scheme {scheme:?} must have 1 to 4 segments")]
    CatalogSegmentCount {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// `required` is 0 or above the segment count.
    #[error("catalog scheme {scheme:?} must require 1 to all of its segments")]
    CatalogRequired {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// A segment has `min > max`.
    #[error("segment {segment} of catalog scheme {scheme:?} has min above max")]
    CatalogSegmentRange {
        /// Key of the scheme.
        scheme: Arc<str>,
        /// Position of the segment.
        segment: usize,
    },
    /// A fixed digit count is outside 1 to 5 or too small for `max`.
    #[error("segment {segment} of catalog scheme {scheme:?} has an invalid digit count")]
    CatalogSegmentDigits {
        /// Key of the scheme.
        scheme: Arc<str>,
        /// Position of the segment.
        segment: usize,
    },
    /// The prefix is longer than 16 bytes or has a char other than an ASCII
    /// letter, `-`, or `_`.
    #[error("catalog scheme {scheme:?} has an invalid prefix")]
    CatalogPrefix {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// `attack_potential.factors` has fewer than `min` or more than `max`
    /// factors.
    #[error("attack_potential must declare {min} to {max} factors")]
    FeasibilityFactorCount {
        /// Fewest factors.
        min: usize,
        /// Most factors.
        max: usize,
    },
    /// Two attack potential factors share a key.
    #[error("feasibility factor key {0:?} is declared twice")]
    DuplicateFeasibilityFactor(Arc<str>),
    /// An attack potential factor has fewer than `min` or more than `max`
    /// levels.
    #[error("feasibility factor {factor:?} must have {min} to {max} levels")]
    FactorLevelCount {
        /// Key of the factor.
        factor: Arc<str>,
        /// Fewest levels.
        min: usize,
        /// Most levels.
        max: usize,
    },
    /// Two levels of one attack potential factor share a key.
    #[error("factor level key {level:?} is declared twice in feasibility factor {factor:?}")]
    DuplicateFactorLevel {
        /// Key of the factor.
        factor: Arc<str>,
        /// The repeated factor level key.
        level: Arc<str>,
    },
    /// A factor level declares a point score above `max`.
    #[error("factor level {level:?} of feasibility factor {factor:?} exceeds max points {max}")]
    FactorPointsTooLarge {
        /// Key of the factor.
        factor: Arc<str>,
        /// Key of the factor level.
        level: Arc<str>,
        /// Maximum allowed points per factor level.
        max: u16,
    },
    /// `attack_potential.bands` has fewer than `min` or more than `max` bands.
    #[error("attack_potential must declare {min} to {max} score bands")]
    PotentialBandCount {
        /// Fewest bands.
        min: usize,
        /// Most bands.
        max: usize,
    },
    /// A score band in `attack_potential.bands` does not form a contiguous,
    /// non-overlapping partition of `0..` ending with an open-ended band.
    #[error("score band {band} in attack_potential has invalid or non-contiguous range")]
    PotentialBandRange {
        /// Position of the invalid band.
        band: usize,
    },
    /// A score band in `attack_potential.bands` names a feasibility level
    /// that is not declared in `feasibility_levels`.
    #[error(
        "score band {band} in attack_potential names unknown feasibility level {feasibility:?}"
    )]
    UnknownBandFeasibilityLevel {
        /// Position of the band.
        band: usize,
        /// The unknown feasibility level key, at most 64 bytes of it (cut like
        /// [`InvalidKey`](Self::InvalidKey)'s).
        feasibility: Arc<str>,
    },
    /// More than `max` interface kinds are declared.
    #[error("more than {max} interface kinds")]
    TooManyInterfaceKinds {
        /// The limit.
        max: usize,
    },
    /// Two interface kinds share a key.
    #[error("interface kind key {0:?} is declared twice")]
    DuplicateInterfaceKind(Arc<str>),
    /// More than `max` attack types are declared.
    #[error("more than {max} attack types")]
    TooManyAttackTypes {
        /// The limit.
        max: usize,
    },
    /// Two attack types share a key.
    #[error("attack type key {0:?} is declared twice")]
    DuplicateAttackType(Arc<str>),
    /// More than `max` language translations are declared in a pack.
    #[error("more than {max} pack translations")]
    TooManyTranslations {
        /// The limit.
        max: usize,
    },
}

/// Errors from saving and loading: [`Project::save`](crate::Project::save),
/// [`Project::load`](crate::Project::load), [`OpLogWriter`](crate::OpLogWriter)
/// and [`OpLogReader`](crate::OpLogReader).
///
/// No variant carries decoded text except vocabulary keys, which are capped
/// at 64 bytes and printed with `{:?}` (escaped). [`Decode`](Self::Decode)
/// keeps only the position of a JSON error and drops the parser's message,
/// because that message can quote input such as an entity name.
///
/// Where it failed: [`Decode`](Self::Decode) and
/// [`TruncatedRecord`](Self::TruncatedRecord) carry their line. For any
/// other error from an op log record, [`OpLogReader::line`](crate::OpLogReader::line)
/// gives the line of the record that failed (the iterator stops there).
/// Snapshot errors other than `Decode` carry no position.
///
/// Not `PartialEq` (it can hold an [`std::io::Error`]); match it with
/// `matches!`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{Methodology, Project, StoreError};
///
/// # fn main() -> Result<(), aegis_model::PackError> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let err = Project::load(&b"{\"format\":\"spreadsheet\"}"[..], Arc::clone(&m)).err();
/// assert!(matches!(err, Some(StoreError::UnsupportedFormat { version: None })));
/// let newer = br#"{"format":"aegis-project","version":2}"#;
/// let err = Project::load(&newer[..], m).err();
/// assert!(matches!(err, Some(StoreError::UnsupportedFormat { version: Some(2) })));
/// # Ok(())
/// # }
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// Reading or writing failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The bytes are not valid JSON, or the JSON has the wrong shape (a
    /// missing, unknown, repeated, or mistyped field; trailing data; a
    /// blank line or a `\r` in an op log). Lines and columns are 1-based;
    /// a column of 0 means the end of the line.
    #[error("malformed input at line {line}, column {column}")]
    Decode {
        /// Line of the error.
        line: u64,
        /// Column of the error.
        column: u64,
    },
    /// A decode cap was exceeded, before any model validation ran.
    #[error("{0}")]
    LimitExceeded(Limit),
    /// The `format` string or the format `version` is not one this build
    /// reads. `version` is the file's format version when that is an
    /// integer above 1 (most likely a file from a newer application: the
    /// user should update), and `None` when the file is not a valid file of
    /// the expected kind at all (wrong format name, or a version of 0, a
    /// negative or a fractional number: the user picked the wrong file).
    #[error("{}", unsupported_format_text(*.version))]
    UnsupportedFormat {
        /// The file's format version, if it is an integer above 1.
        version: Option<u64>,
    },
    /// The file was written under another methodology (its key differs).
    #[error("file was written for methodology {found:?}, expected {expected:?}")]
    MethodologyMismatch {
        /// Key of the supplied methodology.
        expected: Arc<str>,
        /// Key in the file.
        found: Arc<str>,
    },
    /// A vocabulary key in the file is not in the supplied methodology.
    ///
    /// Likely causes: the methodology pack no longer defines the key (the
    /// file needs a pack version that does), or the file is corrupt. The
    /// message names only the vocabulary and the key; an application should
    /// add that hint for its users.
    #[error("unknown {kind} key {key:?}")]
    UnknownKey {
        /// Which vocabulary the key was looked up in.
        kind: VocabKind,
        /// The key, at most 64 bytes.
        key: Arc<str>,
    },
    /// An ID is not a hyphenated lowercase RFC 9562 version 7 UUID.
    #[error("invalid ID: not a hyphenated lowercase UUIDv7")]
    InvalidId,
    /// A catalog ref's text does not match its scheme's syntax.
    #[error("catalog ref does not match the syntax of scheme {scheme:?}")]
    InvalidCatalogRef {
        /// Key of the scheme.
        scheme: Arc<str>,
    },
    /// A property set, an impact map, or a step feasibility map names the
    /// same key twice.
    #[error("a key is repeated in a set or map")]
    DuplicateEntry,
    /// An op log record has the wrong sequence number: a gap, a repeat, or
    /// a step back.
    #[error("expected sequence number {expected}, found {found}")]
    Sequence {
        /// The next number in the run.
        expected: u64,
        /// The number in the record.
        found: u64,
    },
    /// The last line of an op log does not end in `\n`, for example
    /// because a write was cut short.
    #[error("record at line {line} does not end in a newline")]
    TruncatedRecord {
        /// Line of the incomplete record.
        line: u64,
    },
    /// The op log is empty: it has no header line.
    #[error("op log has no header line")]
    MissingHeader,
    /// The op cannot be written so that it reads back the same: an index
    /// is not in the writer's methodology, an impact category is repeated,
    /// a list or name is over its decode cap, the record would exceed the
    /// record cap, or the sequence number would overflow. Nothing was
    /// written.
    #[error("op cannot be encoded under this methodology")]
    UnencodableOp,
    /// A workspace file has a path that is not in the workspace layout,
    /// or breaks a path rule (empty, a leading `/`, a `\\`, a `.` or `..`
    /// segment, or a byte outside lower-case ASCII letters, digits, `-`,
    /// `.` and `/`). The path is cut to 256 bytes.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let files = [("notes.txt", &b"x"[..])];
    /// let err = Project::from_workspace_files(files, m).err();
    /// assert!(matches!(err, Some(StoreError::WorkspacePath { path }) if path == "notes.txt"));
    /// # Ok(())
    /// # }
    /// ```
    #[error("unknown workspace file {path:?}")]
    WorkspacePath {
        /// The path, at most 256 bytes.
        path: String,
    },
    /// The same workspace path was given twice.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let files = [("aegis.json", &b"{}"[..]), ("aegis.json", &b"{}"[..])];
    /// let err = Project::from_workspace_files(files, m).err();
    /// assert!(matches!(err, Some(StoreError::WorkspaceDuplicatePath { path }) if path == "aegis.json"));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace file {path:?} is given twice")]
    WorkspaceDuplicatePath {
        /// The path, at most 256 bytes.
        path: String,
    },
    /// A workspace file has a top-level key that is neither part of the
    /// envelope nor one of the keys its kind holds.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let text = br#"{"format":"aegis-workspace","version":1,"kind":"workspace","extra":1}"#;
    /// let files = [("aegis.json", &text[..])];
    /// let err = Project::from_workspace_files(files, m).err();
    /// assert!(matches!(err, Some(StoreError::WorkspaceUnknownField { field, .. }) if field == "extra"));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace file {path:?} has an unknown field {field:?}")]
    WorkspaceUnknownField {
        /// The path, at most 256 bytes.
        path: String,
        /// The key, at most 64 bytes.
        field: String,
    },
    /// A tree file does not hold exactly one tree, or its file name is not
    /// that tree's visual ID in lower case.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let head = br#"{"format":"aegis-workspace","version":1,"kind":"workspace"}"#;
    /// let tree = br#"{"format":"aegis-workspace","version":1,"kind":"tree","attack_trees":[]}"#;
    /// let files = [("aegis.json", &head[..]), ("systems/main/trees/at00.json", &tree[..])];
    /// let err = Project::from_workspace_files(files, m).err();
    /// assert!(matches!(err, Some(StoreError::WorkspaceTreeName { .. })));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace tree file {path:?} does not hold one tree named by its visual ID")]
    WorkspaceTreeName {
        /// The path, at most 256 bytes.
        path: String,
    },
    /// `aegis.json` names another methodology than the one supplied.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let text = br#"{"format":"aegis-workspace","version":1,"kind":"workspace",
    ///     "methodology":{"key":"other","version":"1"}}"#;
    /// let err = Project::from_workspace_files([("aegis.json", &text[..])], m).err();
    /// assert!(matches!(err, Some(StoreError::WorkspaceMethodology { path }) if path == "aegis.json"));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace file {path:?} is for another methodology")]
    WorkspaceMethodology {
        /// The path, always `aegis.json`.
        path: String,
    },
    /// The joined files were refused by [`Project::load`](crate::Project::load).
    /// `path` is the file that supplied the data the error is about, or
    /// `aegis.json` when that cannot be told.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let text = br#"{"format":"aegis-workspace","version":1,"kind":"workspace",
    ///     "methodology":{"key":"iso21434-r155","version":"1"},"languages":[5]}"#;
    /// let err = Project::from_workspace_files([("aegis.json", &text[..])], m).err();
    /// assert!(matches!(err, Some(StoreError::InWorkspace { path, .. }) if path == "aegis.json"));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace file {path:?}: {source}")]
    InWorkspace {
        /// The path, at most 256 bytes.
        path: String,
        /// What `Project::load` reported.
        source: Box<StoreError>,
    },
    /// A workspace file's envelope is wrong: see [`WorkspaceProblem`].
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError, WorkspaceProblem};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let files = [("aegis.json", &b"nope"[..])];
    /// let err = Project::from_workspace_files(files, m).err();
    /// assert!(matches!(
    ///     err,
    ///     Some(StoreError::WorkspaceEnvelope { problem: WorkspaceProblem::Json, .. })
    /// ));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace file {path:?}: {problem}")]
    WorkspaceEnvelope {
        /// The path of the file, at most 256 bytes.
        path: String,
        /// What is wrong with it.
        problem: WorkspaceProblem,
    },
    /// A required workspace file is absent (`aegis.json`).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, Project, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let err = Project::from_workspace_files(Vec::<(&str, &[u8])>::new(), m).err();
    /// assert!(matches!(err, Some(StoreError::WorkspaceMissing { path }) if path == "aegis.json"));
    /// # Ok(())
    /// # }
    /// ```
    #[error("workspace file {path:?} is missing")]
    WorkspaceMissing {
        /// The path of the missing file.
        path: String,
    },
    /// The decoded data broke a model rule (see [`ModelError`]).
    #[error("model rejected the data: {0}")]
    Model(#[from] ModelError),
}

/// What is wrong with the envelope of a workspace file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WorkspaceProblem {
    /// `format` is not `aegis-workspace`.
    #[error("wrong format name")]
    Format,
    /// `version` is not 1.
    #[error("unsupported version")]
    Version,
    /// `kind` is not the kind that belongs to the path.
    #[error("kind does not match the path")]
    Kind,
    /// The bytes are not a JSON object of the expected shape.
    #[error("not a JSON object of the expected shape")]
    Json,
}

/// Errors from reading or writing a methodology pack:
/// [`MethodologyDef::read_pack`](crate::MethodologyDef::read_pack),
/// [`MethodologyDef::write_pack`](crate::MethodologyDef::write_pack),
/// [`Methodology::read_pack`](crate::Methodology::read_pack), and
/// [`Methodology::iso21434_r155`](crate::Methodology::iso21434_r155).
///
/// Decoding is capped before [`Methodology::new`](crate::Methodology::new)
/// runs, so a hostile pack gives a typed error and bounded memory, never a
/// panic. [`Decode`](Self::Decode) keeps only the position of a JSON error,
/// because the parser's message can quote input.
///
/// Not `PartialEq` (it can hold an [`std::io::Error`]); match it with
/// `matches!`.
///
/// ```
/// use aegis_model::{Limit, Methodology, PackError};
///
/// let newer = br#"{"format":"aegis-methodology-pack","version":2}"#;
/// let err = Methodology::read_pack(&newer[..]).err();
/// assert!(matches!(err, Some(PackError::UnsupportedFormat { version: Some(2) })));
///
/// let label = "x".repeat(2000);
/// let text = format!(
///     r#"{{"format":"aegis-methodology-pack","version":1,
///         "methodology":{{"key":"k","version":"1","label":"{label}"}}}}"#
/// );
/// let err = Methodology::read_pack(text.as_bytes()).err();
/// assert!(matches!(err, Some(PackError::LimitExceeded(Limit::LabelBytes { max: 1024 }))));
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PackError {
    /// Reading or writing failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The bytes are not valid JSON, or the JSON has the wrong shape (a
    /// missing, unknown, repeated, or mistyped field; a number out of its
    /// field's range; trailing data). Lines and columns are 1-based; a
    /// column of 0 means the end of the line (for example an empty input).
    #[error("malformed methodology pack at line {line}, column {column}")]
    Decode {
        /// Line of the error.
        line: u64,
        /// Column of the error.
        column: u64,
    },
    /// A decode cap was exceeded, before any model validation ran.
    #[error("{0}")]
    LimitExceeded(Limit),
    /// The `format` string or the format `version` is not one this build
    /// reads; same meaning and text as
    /// [`StoreError::UnsupportedFormat`]: `version` is the file's format
    /// version when that is an integer above 1 (a newer application wrote
    /// it), else `None` (not a pack at all).
    #[error("{}", unsupported_format_text(*.version))]
    UnsupportedFormat {
        /// The file's format version, if it is an integer above 1.
        version: Option<u64>,
    },
    /// The pack decoded, but its definition breaks a methodology rule.
    #[error("invalid methodology: {0}")]
    Methodology(#[from] MethodologyError),
}

/// The message for [`StoreError::UnsupportedFormat`] and
/// [`PackError::UnsupportedFormat`]. Time O(1).
fn unsupported_format_text(version: Option<u64>) -> String {
    match version {
        Some(v) => format!(
            "file format version {v} is not supported: this build reads version 1, \
             a newer application may read it"
        ),
        None => "not a supported AegisTARA file: wrong format name or format version".into(),
    }
}

/// Formats IDs as `a -> b -> c`; with `close`, the first ID is repeated at
/// the end. Time O(path length).
fn path_text(path: &[NodeId], close: bool) -> String {
    let closing = if close { path.first() } else { None };
    let mut out = String::new();
    for n in path.iter().chain(closing) {
        if !out.is_empty() {
            out.push_str(" -> ");
        }
        out.push_str(&n.to_string());
    }
    out
}
