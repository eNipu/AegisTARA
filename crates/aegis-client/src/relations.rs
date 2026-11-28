//! The relation registry (NFR-12, D30, architecture III.9.2): one row per
//! link that exists in the model. `Links` columns, the inspector, traceability
//! views and reports all read the same table. Data level only.

use aegis_model::{ItemRef, NodeIdx, Project, ThreatScenario, TreeIdx, TreeRole};

use crate::commands::Edit;
use crate::view::Selection;

/// A stable key naming one relation row, used in settings and report profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelationKey(&'static str);

type Read = fn(&Project, Selection) -> Vec<Selection>;
type Link = Option<fn(Selection, Selection) -> Option<Edit>>;

impl RelationKey {
    /// The lowercase key, also used as the Fluent message id.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
    pub const ASSET_COMPONENT: Self = Self("asset-component");
    pub const THREAT_ASSET: Self = Self("threat-asset");
    pub const THREAT_DAMAGES: Self = Self("threat-damages");
    pub const THREAT_TREE: Self = Self("threat-tree");
    pub const THREAT_GOAL: Self = Self("threat-goal");
    pub const THREAT_CLAIM: Self = Self("threat-claim");
    pub const GOAL_CONTROLS: Self = Self("goal-controls");
    pub const GOAL_RESPONSIBLE: Self = Self("goal-responsible");
    pub const CLAIM_RESPONSIBLE: Self = Self("claim-responsible");
    pub const ASSET_CONDITIONS: Self = Self("asset-conditions");
    pub const COMPONENT_CONDITIONS: Self = Self("component-conditions");
    pub const INTERFACE_CONDITIONS: Self = Self("interface-conditions");
    pub const THREAT_CONDITIONS: Self = Self("threat-conditions");
    pub const TREE_CONDITIONS: Self = Self("tree-conditions");
    pub const GOAL_CONDITIONS: Self = Self("goal-conditions");
    pub const CLAIM_CONDITIONS: Self = Self("claim-conditions");
    pub const STEP_CONDITIONS: Self = Self("step-conditions");
    pub const CLAIM_ASSUMPTIONS: Self = Self("claim-assumptions");
    pub const ASSUMPTION_CONDITIONS: Self = Self("assumption-conditions");
    pub const ASSUMPTION_EXCLUDES: Self = Self("assumption-excludes");
    pub const CONTROL_TARGETS: Self = Self("control-targets");
    pub const CONTROL_BYPASS: Self = Self("control-bypass");
    pub const STEP_VIA: Self = Self("step-via");
    pub const SHARED_STEP_VIA: Self = Self("shared-step-via");
    pub const FRAGMENT_USED_IN: Self = Self("fragment-used-in");
    pub const SHARED_STEP_USED_IN: Self = Self("shared-step-used-in");
    pub const STEP_MITIGATIONS: Self = Self("step-mitigations");
    pub const DATAFLOW_FROM: Self = Self("dataflow-from");
    pub const DATAFLOW_TO: Self = Self("dataflow-to");
    pub const DATAFLOW_INTERFACE: Self = Self("dataflow-interface");
    pub const INTERFACE_COMPONENT: Self = Self("interface-component");
    pub const BOUNDARY_MEMBERS: Self = Self("boundary-members");
}

/// The kind of element at one end of a relation, distinct from
/// `aegis_model::ElementKind`; keep paths qualified where both are in scope.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Component,
    Interface,
    DataFlow,
    TrustBoundary,
    Asset,
    Damage,
    Threat,
    Tree,
    Step,
    SharedStep,
    Control,
    Goal,
    Claim,
    Party,
    Condition,
    Assumption,
}

impl ElementKind {
    /// Every relation element kind.
    pub const ALL: &'static [Self] = &[
        Self::Component,
        Self::Interface,
        Self::DataFlow,
        Self::TrustBoundary,
        Self::Asset,
        Self::Damage,
        Self::Threat,
        Self::Tree,
        Self::Step,
        Self::SharedStep,
        Self::Control,
        Self::Goal,
        Self::Claim,
        Self::Party,
        Self::Condition,
        Self::Assumption,
    ];

    /// Maps each `Selection` variant (a `TreeNode` is a `Step`) to its kind.
    #[must_use]
    pub fn kind_of(s: Selection) -> Option<Self> {
        Some(match s {
            Selection::Component(_) => Self::Component,
            Selection::Interface(_) => Self::Interface,
            Selection::DataFlow(_) => Self::DataFlow,
            Selection::TrustBoundary(_) => Self::TrustBoundary,
            Selection::Asset(_) => Self::Asset,
            Selection::Damage(_) => Self::Damage,
            Selection::Threat(_) => Self::Threat,
            Selection::Tree(_) => Self::Tree,
            Selection::TreeNode(_, _) => Self::Step,
            Selection::SharedStep(_) => Self::SharedStep,
            Selection::Control(_) => Self::Control,
            Selection::Goal(_) => Self::Goal,
            Selection::Claim(_) => Self::Claim,
            Selection::Party(_) => Self::Party,
            Selection::Condition(_) => Self::Condition,
            Selection::Assumption(_) => Self::Assumption,
        })
    }
}

/// One row of the registry: a single link type in the model.
pub struct Relation {
    /// Stable key, used in settings and report profiles.
    pub key: RelationKey,
    /// Kind of the source element.
    pub from: ElementKind,
    /// Kinds a target may have (control targets and step vias are a
    /// component or an interface).
    pub to: &'static [ElementKind],
    /// Fluent key of the label shown for this relation.
    pub label: &'static str,
    /// Whether a source may have more than one target.
    pub many: bool,
    /// Resolves `from`'s targets; empty for a stale or wrong-kind selection.
    pub read: fn(&Project, Selection) -> Vec<Selection>,
    /// Builds the edit that links a source to a target, when one exists.
    pub link: Option<fn(Selection, Selection) -> Option<Edit>>,
    /// Builds the edit that unlinks a source from a target, when one exists.
    pub unlink: Option<fn(Selection, Selection) -> Option<Edit>>,
}

#[rustfmt::skip]
#[allow(clippy::too_many_arguments)] // mirrors the eight public Relation fields
const fn rel(key: RelationKey, from: ElementKind, to: &'static [ElementKind], label: &'static str, many: bool, read: Read, link: Link, unlink: Link) -> Relation {
    Relation { key, from, to, label, many, read, link, unlink }
}

/// A component or an interface: the targets an `ItemRef` can name.
const ITEM: &[ElementKind] = &[ElementKind::Component, ElementKind::Interface];

/// The target of every condition row.
const CONDITION: &[ElementKind] = &[ElementKind::Condition];

/// The registry: exactly one row per link that exists in the model.
#[rustfmt::skip]
pub static RELATIONS: &[Relation] = &[
    rel(RelationKey::ASSET_COMPONENT, ElementKind::Asset, &[ElementKind::Component], "relation-asset-component", false, read_asset_component, Some(link_asset_component), Some(unlink_asset_component)),
    rel(RelationKey::THREAT_ASSET, ElementKind::Threat, &[ElementKind::Asset], "relation-threat-asset", false, read_threat_asset, None, None),
    rel(RelationKey::THREAT_DAMAGES, ElementKind::Threat, &[ElementKind::Damage], "relation-threat-damages", true, read_threat_damages, Some(link_threat_damages), Some(unlink_threat_damages)),
    rel(RelationKey::THREAT_TREE, ElementKind::Threat, &[ElementKind::Tree], "relation-threat-tree", false, read_threat_tree, Some(link_threat_tree), Some(unlink_threat_tree)),
    rel(RelationKey::THREAT_GOAL, ElementKind::Threat, &[ElementKind::Goal], "relation-threat-goal", true, read_threat_goal, Some(link_threat_goal), Some(unlink_threat_goal)),
    rel(RelationKey::THREAT_CLAIM, ElementKind::Threat, &[ElementKind::Claim], "relation-threat-claim", false, read_threat_claim, Some(link_threat_claim), Some(unlink_threat_claim)),
    rel(RelationKey::GOAL_CONTROLS, ElementKind::Goal, &[ElementKind::Control], "relation-goal-controls", true, read_goal_controls, Some(link_goal_controls), Some(unlink_goal_controls)),
    rel(RelationKey::GOAL_RESPONSIBLE, ElementKind::Goal, &[ElementKind::Party], "relation-goal-responsible", true, read_goal_responsible, Some(link_goal_responsible), Some(unlink_goal_responsible)),
    rel(RelationKey::CLAIM_RESPONSIBLE, ElementKind::Claim, &[ElementKind::Party], "relation-claim-responsible", true, read_claim_responsible, Some(link_claim_responsible), Some(unlink_claim_responsible)),
    rel(RelationKey::CONTROL_TARGETS, ElementKind::Control, ITEM, "relation-control-targets", true, read_control_targets, Some(link_control_targets), Some(unlink_control_targets)),
    rel(RelationKey::CONTROL_BYPASS, ElementKind::Control, &[ElementKind::Tree], "relation-control-bypass", false, read_control_bypass, Some(link_control_bypass), Some(unlink_control_bypass)),
    rel(RelationKey::STEP_VIA, ElementKind::Step, ITEM, "relation-step-via", false, read_step_via, Some(link_step_via), Some(unlink_step_via)),
    rel(RelationKey::SHARED_STEP_VIA, ElementKind::SharedStep, ITEM, "relation-step-via", false, read_shared_step_via, Some(link_shared_step_via), Some(unlink_shared_step_via)),
    rel(RelationKey::SHARED_STEP_USED_IN, ElementKind::SharedStep, &[ElementKind::Step], "inspector-used-in", true, read_shared_step_used_in, None, None),
    rel(RelationKey::FRAGMENT_USED_IN, ElementKind::Tree, &[ElementKind::Step], "inspector-used-in", true, read_fragment_used_in, None, None),
    rel(RelationKey::STEP_MITIGATIONS, ElementKind::Step, &[ElementKind::Control], "relation-step-mitigations", true, read_step_mitigations, None, None),
    rel(RelationKey::DATAFLOW_FROM, ElementKind::DataFlow, &[ElementKind::Component], "relation-dataflow-from", false, read_dataflow_from, None, None),
    rel(RelationKey::DATAFLOW_TO, ElementKind::DataFlow, &[ElementKind::Component], "relation-dataflow-to", false, read_dataflow_to, None, None),
    rel(RelationKey::DATAFLOW_INTERFACE, ElementKind::DataFlow, &[ElementKind::Interface], "relation-dataflow-interface", false, read_dataflow_interface, Some(link_dataflow_interface), Some(unlink_dataflow_interface)),
    rel(RelationKey::INTERFACE_COMPONENT, ElementKind::Interface, &[ElementKind::Component], "relation-interface-component", false, read_interface_component, None, None),
    rel(RelationKey::BOUNDARY_MEMBERS, ElementKind::TrustBoundary, &[ElementKind::Component], "relation-boundary-members", true, read_boundary_members, Some(link_boundary_members), Some(unlink_boundary_members)),
    rel(RelationKey::ASSET_CONDITIONS, ElementKind::Asset, CONDITION, "relation-asset-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::COMPONENT_CONDITIONS, ElementKind::Component, CONDITION, "relation-component-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::INTERFACE_CONDITIONS, ElementKind::Interface, CONDITION, "relation-interface-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::THREAT_CONDITIONS, ElementKind::Threat, CONDITION, "relation-threat-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::TREE_CONDITIONS, ElementKind::Tree, CONDITION, "relation-tree-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::GOAL_CONDITIONS, ElementKind::Goal, CONDITION, "relation-goal-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::CLAIM_CONDITIONS, ElementKind::Claim, CONDITION, "relation-claim-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::STEP_CONDITIONS, ElementKind::Step, CONDITION, "relation-step-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::CLAIM_ASSUMPTIONS, ElementKind::Claim, &[ElementKind::Assumption], "relation-claim-assumptions", true, read_claim_assumptions, Some(link_claim_assumptions), Some(unlink_claim_assumptions)),
    rel(RelationKey::ASSUMPTION_CONDITIONS, ElementKind::Assumption, CONDITION, "relation-assumption-conditions", true, read_conditions, Some(need_condition), Some(unneed_condition)),
    rel(RelationKey::ASSUMPTION_EXCLUDES, ElementKind::Assumption, &[ElementKind::Tree, ElementKind::Step], "relation-assumption-excludes", true, read_assumption_excludes, Some(link_assumption_excludes), Some(unlink_assumption_excludes)),
];

/// The registry row for `key`, or `None` if the key is unknown.
#[must_use]
pub fn relation(key: RelationKey) -> Option<&'static Relation> {
    RELATIONS.iter().find(|row| row.key == key)
}

/// Forward targets of `from`; empty for an unknown key, wrong-kind selection
/// or stale index.
#[must_use]
pub fn related(project: &Project, key: RelationKey, from: Selection) -> Vec<Selection> {
    match relation(key) {
        Some(row) => (row.read)(project, from),
        None => Vec::new(),
    }
}

/// Every element of `kind` in the project, as a stable [`Selection`]; empty for
/// a kind with no elements. A tree node (step) is `(TreeId, NodeId)` from every
/// tree's `iter()`. O(elements).
#[must_use]
#[rustfmt::skip]
pub fn elements_of_kind(project: &Project, kind: ElementKind) -> Vec<Selection> {
    match kind {
        ElementKind::Asset => project.assets().map(|(_, id, _)| Selection::Asset(id)).collect(),
        ElementKind::Damage => project.damage_scenarios().map(|(_, id, _)| Selection::Damage(id)).collect(),
        ElementKind::Threat => project.threat_scenarios().map(|(_, id, _)| Selection::Threat(id)).collect(),
        ElementKind::Tree => project.attack_trees().map(|(_, id, _)| Selection::Tree(id)).collect(),
        ElementKind::Step => project.attack_trees().flat_map(|(_, id, t)| t.iter().filter(|(_, _, n)| n.gate_kind().is_none()).map(move |(_, n, _)| Selection::TreeNode(id, n))).collect(),
        ElementKind::SharedStep => project.shared_steps().map(|(_, id, _)| Selection::SharedStep(id)).collect(),
        ElementKind::Control => project.security_controls().map(|(_, id, _)| Selection::Control(id)).collect(),
        ElementKind::Goal => project.cybersecurity_goals().map(|(_, id, _)| Selection::Goal(id)).collect(),
        ElementKind::Claim => project.cybersecurity_claims().map(|(_, id, _)| Selection::Claim(id)).collect(),
        ElementKind::Party => project.parties().map(|(_, id, _)| Selection::Party(id)).collect(),
        ElementKind::Condition => project.conditions().map(|(_, id, _)| Selection::Condition(id)).collect(),
        ElementKind::Assumption => project.assumptions().map(|(_, id, _)| Selection::Assumption(id)).collect(),
        ElementKind::Component => project.components().map(|(_, id, _)| Selection::Component(id)).collect(),
        ElementKind::Interface => project.interfaces().map(|(_, id, _)| Selection::Interface(id)).collect(),
        ElementKind::DataFlow => project.data_flows().map(|(_, id, _)| Selection::DataFlow(id)).collect(),
        ElementKind::TrustBoundary => project.trust_boundaries().map(|(_, id, _)| Selection::TrustBoundary(id)).collect(),
    }
}

/// Sources that link to `to` under `key`. Scans every source of the row's
/// `from` kind via [`elements_of_kind`] and keeps those whose `read` contains
/// `to`: O(elements x relations), with no index because the table has tens of
/// rows.
#[must_use]
pub fn reverse_related(project: &Project, key: RelationKey, to: Selection) -> Vec<Selection> {
    let Some(row) = relation(key) else {
        return Vec::new();
    };
    elements_of_kind(project, row.from)
        .into_iter()
        .filter(|s| (row.read)(project, *s).contains(&to))
        .collect()
}

/// Every possible target of `key`: all elements of the row's `to` kind, empty
/// for an unknown key. O(elements).
#[must_use]
pub fn candidates(project: &Project, key: RelationKey) -> Vec<Selection> {
    if let Some(role) = match key {
        RelationKey::THREAT_TREE => Some(TreeRole::Threat),
        RelationKey::CONTROL_BYPASS => Some(TreeRole::Fragment),
        _ => None,
    } {
        return project
            .attack_trees()
            .filter(|(idx, _, _)| project.attack_tree_role(*idx) == Some(role))
            .map(|(_, id, _)| Selection::Tree(id))
            .collect();
    }
    match relation(key) {
        Some(row) => row
            .to
            .iter()
            .flat_map(|&k| elements_of_kind(project, k))
            .collect(),
        None => Vec::new(),
    }
}

/// Every non-empty group for `target`: forward groups (rows whose `from`
/// kind matches) first, then the reverse groups. This is what the inspector
/// reads.
#[must_use]
#[rustfmt::skip]
pub fn linked(project: &Project, target: Selection) -> Vec<(RelationKey, Vec<Selection>)> {
    let kind = ElementKind::kind_of(target);
    let mut out = Vec::new();
    for row in RELATIONS {
        if kind == Some(row.from) {
            let targets = (row.read)(project, target);
            if !targets.is_empty() { out.push((row.key, targets)); }
        }
    }
    for row in RELATIONS {
        let sources = reverse_related(project, row.key, target);
        if !sources.is_empty() { out.push((row.key, sources)); }
    }
    out
}

// -- shared helpers ---------------------------------------------------------

#[rustfmt::skip]
fn item_selection(item: ItemRef) -> Option<Selection> {
    match item { ItemRef::Component(id) => Some(Selection::Component(id)), ItemRef::Interface(id) => Some(Selection::Interface(id)) }
}

#[rustfmt::skip]
fn selection_item(s: Selection) -> Option<ItemRef> {
    match s { Selection::Component(id) => Some(ItemRef::Component(id)), Selection::Interface(id) => Some(ItemRef::Interface(id)), _ => None }
}

#[rustfmt::skip]
fn threat(project: &Project, from: Selection) -> Option<&ThreatScenario> {
    let Selection::Threat(id) = from else { return None };
    project.threat_scenario(project.threat_scenario_idx(&id)?)
}

#[rustfmt::skip]
fn step(project: &Project, from: Selection) -> Option<(TreeIdx, NodeIdx)> {
    let Selection::TreeNode(tree_id, node_id) = from else { return None };
    let tree = project.attack_tree_idx(&tree_id)?;
    Some((tree, project.attack_tree(tree)?.idx_of(&node_id)?))
}

// -- forward reads ----------------------------------------------------------

#[rustfmt::skip]
fn read_asset_component(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Asset(id) = from else { return Vec::new() };
    let Some(asset) = project.asset_idx(&id).and_then(|i| project.asset(i)) else { return Vec::new() };
    asset.component().and_then(|c| project.component_id(c)).map(Selection::Component).into_iter().collect()
}
#[rustfmt::skip]
fn read_threat_asset(project: &Project, from: Selection) -> Vec<Selection> {
    threat(project, from).and_then(|t| project.asset_id(t.asset())).map(Selection::Asset).into_iter().collect()
}
#[rustfmt::skip]
fn read_threat_damages(project: &Project, from: Selection) -> Vec<Selection> {
    threat(project, from).map_or_else(Vec::new, |t| t.damages().iter().filter_map(|&d| project.damage_scenario_id(d)).map(Selection::Damage).collect())
}
#[rustfmt::skip]
fn read_threat_tree(project: &Project, from: Selection) -> Vec<Selection> {
    threat(project, from).and_then(|t| t.attack_tree()).and_then(|t| project.attack_tree_id(t)).map(Selection::Tree).into_iter().collect()
}
#[rustfmt::skip]
fn read_threat_goal(project: &Project, from: Selection) -> Vec<Selection> {
    threat(project, from)
        .map(|t| {
            t.goals()
                .iter()
                .filter_map(|&g| project.cybersecurity_goal_id(g))
                .map(Selection::Goal)
                .collect()
        })
        .unwrap_or_default()
}
#[rustfmt::skip]
fn read_threat_claim(project: &Project, from: Selection) -> Vec<Selection> {
    threat(project, from).and_then(|t| t.claim()).and_then(|c| project.cybersecurity_claim_id(c)).map(Selection::Claim).into_iter().collect()
}
#[rustfmt::skip]
fn read_goal_controls(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Goal(id) = from else { return Vec::new() };
    let Some(goal) = project.cybersecurity_goal_idx(&id).and_then(|i| project.cybersecurity_goal(i)) else { return Vec::new() };
    goal.controls().iter().filter_map(|&c| project.security_control_id(c)).map(Selection::Control).collect()
}
#[rustfmt::skip]
fn read_goal_responsible(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Goal(id) = from else { return Vec::new() };
    let Some(idx) = project.cybersecurity_goal_idx(&id) else { return Vec::new() };
    project.cybersecurity_goal(idx).map_or_else(Vec::new, |g| g.responsible().iter().filter_map(|&p| project.party_id(p).map(Selection::Party)).collect())
}
#[rustfmt::skip]
fn read_claim_responsible(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Claim(id) = from else { return Vec::new() };
    let Some(idx) = project.cybersecurity_claim_idx(&id) else { return Vec::new() };
    project.cybersecurity_claim(idx).map_or_else(Vec::new, |c| c.responsible().iter().filter_map(|&p| project.party_id(p).map(Selection::Party)).collect())
}
#[rustfmt::skip]
fn read_claim_assumptions(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Claim(id) = from else { return Vec::new() };
    let Some(idx) = project.cybersecurity_claim_idx(&id) else { return Vec::new() };
    project.cybersecurity_claim(idx).map_or_else(Vec::new, |c| c.assumptions().iter().filter_map(|&a| project.assumption_id(a).map(Selection::Assumption)).collect())
}
#[rustfmt::skip]
fn read_assumption_excludes(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Assumption(id) = from else { return Vec::new() };
    let Some(a) = project.assumption_idx(&id).and_then(|i| project.assumption(i)) else { return Vec::new() };
    let trees = a.excludes_trees().iter().filter_map(|&t| project.attack_tree_id(t).map(Selection::Tree));
    let steps = a.excludes_steps().iter().map(|s| Selection::TreeNode(s.tree, s.node));
    trees.chain(steps).collect()
}
#[rustfmt::skip]
fn read_control_targets(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Control(id) = from else { return Vec::new() };
    project.security_control_idx(&id).map_or_else(Vec::new, |i| project.control_targets(i).filter_map(item_selection).collect())
}
#[rustfmt::skip]
fn read_control_bypass(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Control(id) = from else { return Vec::new() };
    project.security_control_idx(&id).and_then(|i| project.security_control(i)).and_then(|c| c.bypass()).and_then(|t| project.attack_tree_id(t)).map(Selection::Tree).into_iter().collect()
}
#[rustfmt::skip]
fn read_step_via(project: &Project, from: Selection) -> Vec<Selection> {
    step(project, from)
        .filter(|(tree, node)| project.step_shared(*tree, *node).is_none())
        .and_then(|(tree, node)| project.step_via(tree, node))
        .and_then(item_selection)
        .into_iter()
        .collect()
}

fn read_shared_step_via(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::SharedStep(id) = from else {
        return Vec::new();
    };
    project
        .shared_step_idx(&id)
        .and_then(|idx| project.shared_step_via(idx))
        .and_then(item_selection)
        .into_iter()
        .collect()
}

fn read_fragment_used_in(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Tree(id) = from else {
        return Vec::new();
    };
    let Some(idx) = project.attack_tree_idx(&id) else {
        return Vec::new();
    };
    if project.attack_tree_role(idx) != Some(TreeRole::Fragment) {
        return Vec::new();
    }
    project
        .fragment_users(idx)
        .into_iter()
        .map(|s| Selection::TreeNode(s.tree, s.node))
        .collect()
}

fn read_shared_step_used_in(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::SharedStep(id) = from else {
        return Vec::new();
    };
    let Some(shared) = project.shared_step_idx(&id) else {
        return Vec::new();
    };
    let mut users = Vec::new();
    for (tree_idx, tree_id, tree) in project.attack_trees() {
        for (node_idx, node_id, _) in tree.iter() {
            if project.step_shared(tree_idx, node_idx) == Some(shared) {
                users.push((tree_id, node_id));
            }
        }
    }
    users.sort_by_key(|(tree, node)| (*tree, *node));
    users
        .into_iter()
        .map(|(tree, node)| Selection::TreeNode(tree, node))
        .collect()
}
#[rustfmt::skip]
fn read_step_mitigations(project: &Project, from: Selection) -> Vec<Selection> {
    step(project, from).map_or_else(Vec::new, |(tree, node)| project.step_mitigations(tree, node).iter().filter_map(|m| project.security_control_id(m.control())).map(Selection::Control).collect())
}
#[rustfmt::skip]
fn read_dataflow_from(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::DataFlow(id) = from else { return Vec::new() };
    project.data_flow_idx(&id).and_then(|i| project.data_flow(i)).map_or_else(Vec::new, |f| project.component_id(f.from()).map(Selection::Component).into_iter().collect())
}
#[rustfmt::skip]
fn read_dataflow_to(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::DataFlow(id) = from else { return Vec::new() };
    project.data_flow_idx(&id).and_then(|i| project.data_flow(i)).map_or_else(Vec::new, |f| project.component_id(f.to()).map(Selection::Component).into_iter().collect())
}

#[rustfmt::skip]
fn read_dataflow_interface(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::DataFlow(id) = from else { return Vec::new() };
    project.data_flow_idx(&id).and_then(|i| project.data_flow(i)).and_then(|f| f.interface()).and_then(|i| project.interface_id(i)).map(Selection::Interface).into_iter().collect()
}
#[rustfmt::skip]
fn read_interface_component(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Interface(id) = from else { return Vec::new() };
    project.interface_idx(&id).and_then(|i| project.interface(i)).and_then(|f| project.component_id(f.component())).map(Selection::Component).into_iter().collect()
}
#[rustfmt::skip]
fn read_boundary_members(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::TrustBoundary(id) = from else { return Vec::new() };
    project.trust_boundary_idx(&id).and_then(|i| project.trust_boundary(i)).map_or_else(Vec::new, |b| b.components().iter().filter_map(|&c| project.component_id(c)).map(Selection::Component).collect())
}

/// Where a selection keeps its condition links: an element, or a step.
enum ConditionHolder {
    Element(aegis_model::ElementRef),
    Step(aegis_model::TreeId, aegis_model::NodeId),
}

fn condition_holder(s: Selection) -> Option<ConditionHolder> {
    use aegis_model::ElementRef as E;
    Some(match s {
        Selection::Asset(id) => ConditionHolder::Element(E::Asset(id)),
        Selection::Component(id) => ConditionHolder::Element(E::Component(id)),
        Selection::Interface(id) => ConditionHolder::Element(E::Interface(id)),
        Selection::Threat(id) => ConditionHolder::Element(E::Threat(id)),
        Selection::Tree(id) => ConditionHolder::Element(E::Tree(id)),
        Selection::Goal(id) => ConditionHolder::Element(E::Goal(id)),
        Selection::Claim(id) => ConditionHolder::Element(E::Claim(id)),
        Selection::Assumption(id) => ConditionHolder::Element(E::Assumption(id)),
        Selection::TreeNode(tree, node) => ConditionHolder::Step(tree, node),
        _ => return None,
    })
}

/// The conditions an element or step needs; shared by the eight condition rows.
fn read_conditions(project: &Project, from: Selection) -> Vec<Selection> {
    let list = match condition_holder(from) {
        Some(ConditionHolder::Element(e)) => project.condition_link_ids(e),
        Some(ConditionHolder::Step(tree, node)) => project.step_condition_link_ids(tree, node),
        None => return Vec::new(),
    };
    list.iter()
        .filter_map(|&i| project.condition_id(i))
        .map(Selection::Condition)
        .collect()
}

fn condition_edit(from: Selection, to: Selection, linked: bool) -> Option<Edit> {
    let Selection::Condition(condition) = to else {
        return None;
    };
    Some(match condition_holder(from)? {
        ConditionHolder::Element(element) => Edit::SetConditionLink {
            element,
            condition,
            linked,
        },
        ConditionHolder::Step(tree, node) => Edit::SetStepConditionLink {
            tree,
            node,
            condition,
            linked,
        },
    })
}

fn need_condition(from: Selection, to: Selection) -> Option<Edit> {
    condition_edit(from, to, true)
}

fn unneed_condition(from: Selection, to: Selection) -> Option<Edit> {
    condition_edit(from, to, false)
}

// -- link / unlink ----------------------------------------------------------

#[rustfmt::skip]
fn link_asset_component(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Asset(a), Selection::Component(c)) => Some(Edit::SetAssetComponent { asset: a, component: Some(c) }), _ => None }
}
#[rustfmt::skip]
fn unlink_asset_component(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Asset(a), Selection::Component(_)) => Some(Edit::SetAssetComponent { asset: a, component: None }), _ => None }
}
#[rustfmt::skip]
fn link_threat_damages(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Damage(d)) => Some(Edit::LinkDamage { threat: t, damage: d, linked: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_threat_damages(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Damage(d)) => Some(Edit::LinkDamage { threat: t, damage: d, linked: false }), _ => None }
}
#[rustfmt::skip]
fn link_threat_tree(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Tree(tree)) => Some(Edit::SetThreatTree { threat: t, tree: Some(tree) }), _ => None }
}
#[rustfmt::skip]
fn unlink_threat_tree(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Tree(_)) => Some(Edit::SetThreatTree { threat: t, tree: None }), _ => None }
}
#[rustfmt::skip]
fn link_threat_goal(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Goal(g)) => Some(Edit::LinkThreatGoal { threat: t, goal: g }), _ => None }
}
#[rustfmt::skip]
fn unlink_threat_goal(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Goal(g)) => Some(Edit::UnlinkThreatGoal { threat: t, goal: g }), _ => None }
}
#[rustfmt::skip]
fn link_threat_claim(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Claim(c)) => Some(Edit::SetThreatClaim { threat: t, claim: Some(c) }), _ => None }
}
#[rustfmt::skip]
fn unlink_threat_claim(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Claim(_)) => Some(Edit::SetThreatClaim { threat: t, claim: None }), _ => None }
}
#[rustfmt::skip]
fn link_goal_controls(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Goal(g), Selection::Control(c)) => Some(Edit::LinkGoalControl { goal: g, control: c, linked: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_goal_controls(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Goal(g), Selection::Control(c)) => Some(Edit::LinkGoalControl { goal: g, control: c, linked: false }), _ => None }
}
#[rustfmt::skip]
fn link_goal_responsible(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Goal(g), Selection::Party(p)) => Some(Edit::LinkGoalParty { goal: g, party: p, linked: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_goal_responsible(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Goal(g), Selection::Party(p)) => Some(Edit::LinkGoalParty { goal: g, party: p, linked: false }), _ => None }
}
#[rustfmt::skip]
fn link_claim_responsible(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Claim(c), Selection::Party(p)) => Some(Edit::LinkClaimParty { claim: c, party: p, linked: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_claim_responsible(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Claim(c), Selection::Party(p)) => Some(Edit::LinkClaimParty { claim: c, party: p, linked: false }), _ => None }
}
#[rustfmt::skip]
fn link_control_targets(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::Control(c), Some(target)) => Some(Edit::ApplyControl { control: c, target }), _ => None }
}
#[rustfmt::skip]
fn unlink_control_targets(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::Control(c), Some(target)) => Some(Edit::UnapplyControl { control: c, target }), _ => None }
}
#[rustfmt::skip]
fn link_control_bypass(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Control(control), Selection::Tree(tree)) => Some(Edit::SetControlBypass { control, tree: Some(tree) }), _ => None }
}
#[rustfmt::skip]
fn unlink_control_bypass(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Control(control), Selection::Tree(_)) => Some(Edit::SetControlBypass { control, tree: None }), _ => None }
}
#[rustfmt::skip]
fn link_step_via(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::TreeNode(tree, node), Some(via)) => Some(Edit::SetStepVia { tree, node, via: Some(via) }), _ => None }
}
#[rustfmt::skip]
fn unlink_step_via(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::TreeNode(tree, node), Some(_)) => Some(Edit::SetStepVia { tree, node, via: None }), _ => None }
}
fn link_shared_step_via(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) {
        (Selection::SharedStep(id), Some(via)) => Some(Edit::SetSharedStepVia {
            shared_step: id,
            via: Some(via),
        }),
        _ => None,
    }
}
fn unlink_shared_step_via(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) {
        (Selection::SharedStep(id), Some(_)) => Some(Edit::SetSharedStepVia {
            shared_step: id,
            via: None,
        }),
        _ => None,
    }
}

#[rustfmt::skip]
fn link_dataflow_interface(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::DataFlow(f), Selection::Interface(i)) => Some(Edit::SetDataFlowInterface { data_flow: f, interface: Some(i) }), _ => None }
}
#[rustfmt::skip]
fn unlink_dataflow_interface(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::DataFlow(f), Selection::Interface(_)) => Some(Edit::SetDataFlowInterface { data_flow: f, interface: None }), _ => None }
}
#[rustfmt::skip]
fn link_boundary_members(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::TrustBoundary(b), Selection::Component(c)) => Some(Edit::SetTrustBoundaryMember { boundary: b, component: c, member: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_boundary_members(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::TrustBoundary(b), Selection::Component(c)) => Some(Edit::SetTrustBoundaryMember { boundary: b, component: c, member: false }), _ => None }
}
#[rustfmt::skip]
fn link_claim_assumptions(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Claim(claim), Selection::Assumption(assumption)) => Some(Edit::LinkClaimAssumption { claim, assumption, linked: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_claim_assumptions(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Claim(claim), Selection::Assumption(assumption)) => Some(Edit::LinkClaimAssumption { claim, assumption, linked: false }), _ => None }
}
#[rustfmt::skip]
fn link_assumption_excludes(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Assumption(assumption), target @ (Selection::Tree(_) | Selection::TreeNode(_, _))) => Some(Edit::SetAssumptionExclude { assumption, target, excluded: true }), _ => None }
}
#[rustfmt::skip]
fn unlink_assumption_excludes(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Assumption(assumption), target @ (Selection::Tree(_) | Selection::TreeNode(_, _))) => Some(Edit::SetAssumptionExclude { assumption, target, excluded: false }), _ => None }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use aegis_model::{
        AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GoalId, InterfaceId,
        Methodology, NodeId, PartyId, SharedStepId, ThreatId, TreeId, TrustBoundaryId,
    };

    use crate::workspace::{Origin, Workspace};

    use super::*;

    #[test]
    fn relation_keys_are_unique() {
        let keys: HashSet<RelationKey> = RELATIONS.iter().map(|row| row.key).collect();
        assert_eq!(keys.len(), RELATIONS.len(), "a relation key repeats");
    }

    #[test]
    fn kind_of_maps_every_selection_variant() {
        let selections = [
            Selection::Component(ComponentId::new_v7()),
            Selection::Interface(InterfaceId::new_v7()),
            Selection::DataFlow(DataFlowId::new_v7()),
            Selection::TrustBoundary(TrustBoundaryId::new_v7()),
            Selection::Asset(AssetId::new_v7()),
            Selection::Damage(DamageId::new_v7()),
            Selection::Threat(ThreatId::new_v7()),
            Selection::Tree(TreeId::new_v7()),
            Selection::TreeNode(TreeId::new_v7(), NodeId::new_v7()),
            Selection::SharedStep(SharedStepId::new_v7()),
            Selection::Control(ControlId::new_v7()),
            Selection::Goal(GoalId::new_v7()),
            Selection::Claim(ClaimId::new_v7()),
            Selection::Party(PartyId::new_v7()),
            Selection::Condition(aegis_model::ConditionId::new_v7()),
            Selection::Assumption(aegis_model::AssumptionId::new_v7()),
        ];
        for selection in selections {
            assert!(ElementKind::kind_of(selection).is_some(), "{selection:?}");
        }
    }

    #[test]
    fn labels_are_non_empty() {
        for row in RELATIONS {
            assert!(!row.label.is_empty(), "{:?}", row.key);
        }
    }

    #[test]
    fn candidates_lists_every_target_of_the_row() {
        let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
        let mut ws = Workspace::empty(m);
        let (d1, d2, c1) = (DamageId::new_v7(), DamageId::new_v7(), ControlId::new_v7());
        for edit in [
            Edit::AddDamage {
                id: d1,
                name: "Crash".into(),
            },
            Edit::AddDamage {
                id: d2,
                name: "Brake".into(),
            },
            Edit::AddControl {
                id: c1,
                name: "ABS".into(),
            },
        ] {
            let ops = edit.to_ops(ws.project(), "alice", 1).expect("ops");
            ws.apply(ops, Origin::Edit).expect("applies");
        }
        let damages: Vec<Selection> = ws
            .project()
            .damage_scenarios()
            .map(|(_, id, _)| Selection::Damage(id))
            .collect();
        let controls: Vec<Selection> = ws
            .project()
            .security_controls()
            .map(|(_, id, _)| Selection::Control(id))
            .collect();
        assert_eq!(
            candidates(ws.project(), RelationKey::THREAT_DAMAGES),
            damages
        );
        assert_eq!(
            candidates(ws.project(), RelationKey::GOAL_CONTROLS),
            controls
        );
    }
}
