//! The relation registry (NFR-12, D30, architecture III.9.2): one row per
//! link that exists in the model. `Links` columns, the inspector, traceability
//! views and reports all read the same table. Data level only.

use aegis_model::{ItemRef, NodeIdx, Project, ThreatScenario, TreeIdx};

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
    pub const CONTROL_TARGETS: Self = Self("control-targets");
    pub const STEP_VIA: Self = Self("step-via");
    pub const STEP_MITIGATIONS: Self = Self("step-mitigations");
    pub const DATAFLOW_FROM: Self = Self("dataflow-from");
    pub const DATAFLOW_TO: Self = Self("dataflow-to");
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
    Control,
    Goal,
    Claim,
}

impl ElementKind {
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
            Selection::Control(_) => Self::Control,
            Selection::Goal(_) => Self::Goal,
            Selection::Claim(_) => Self::Claim,
        })
    }
}

/// One row of the registry: a single link type in the model.
pub struct Relation {
    /// Stable key, used in settings and report profiles.
    pub key: RelationKey,
    /// Kind of the source element.
    pub from: ElementKind,
    /// Kind of the target element.
    pub to: ElementKind,
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
const fn rel(key: RelationKey, from: ElementKind, to: ElementKind, label: &'static str, many: bool, read: Read, link: Link, unlink: Link) -> Relation {
    Relation { key, from, to, label, many, read, link, unlink }
}

/// The registry: exactly one row per link that exists in the model.
#[rustfmt::skip]
pub static RELATIONS: &[Relation] = &[
    rel(RelationKey::ASSET_COMPONENT, ElementKind::Asset, ElementKind::Component, "relation-asset-component", false, read_asset_component, Some(link_asset_component), Some(unlink_asset_component)),
    rel(RelationKey::THREAT_ASSET, ElementKind::Threat, ElementKind::Asset, "relation-threat-asset", false, read_threat_asset, None, None),
    rel(RelationKey::THREAT_DAMAGES, ElementKind::Threat, ElementKind::Damage, "relation-threat-damages", true, read_threat_damages, Some(link_threat_damages), Some(unlink_threat_damages)),
    rel(RelationKey::THREAT_TREE, ElementKind::Threat, ElementKind::Tree, "relation-threat-tree", false, read_threat_tree, Some(link_threat_tree), Some(unlink_threat_tree)),
    rel(RelationKey::THREAT_GOAL, ElementKind::Threat, ElementKind::Goal, "relation-threat-goal", false, read_threat_goal, Some(link_threat_goal), Some(unlink_threat_goal)),
    rel(RelationKey::THREAT_CLAIM, ElementKind::Threat, ElementKind::Claim, "relation-threat-claim", false, read_threat_claim, Some(link_threat_claim), Some(unlink_threat_claim)),
    rel(RelationKey::GOAL_CONTROLS, ElementKind::Goal, ElementKind::Control, "relation-goal-controls", true, read_goal_controls, Some(link_goal_controls), Some(unlink_goal_controls)),
    rel(RelationKey::CONTROL_TARGETS, ElementKind::Control, ElementKind::Component, "relation-control-targets", true, read_control_targets, Some(link_control_targets), Some(unlink_control_targets)),
    rel(RelationKey::STEP_VIA, ElementKind::Step, ElementKind::Component, "relation-step-via", false, read_step_via, Some(link_step_via), Some(unlink_step_via)),
    rel(RelationKey::STEP_MITIGATIONS, ElementKind::Step, ElementKind::Control, "relation-step-mitigations", true, read_step_mitigations, None, None),
    rel(RelationKey::DATAFLOW_FROM, ElementKind::DataFlow, ElementKind::Component, "relation-dataflow-from", false, read_dataflow_from, None, None),
    rel(RelationKey::DATAFLOW_TO, ElementKind::DataFlow, ElementKind::Component, "relation-dataflow-to", false, read_dataflow_to, None, None),
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

/// Sources that link to `to` under `key`. Scans every source of the row's
/// `from` kind and keeps those whose `read` contains `to`: O(elements x
/// relations), with no index because the table has tens of rows.
#[must_use]
#[rustfmt::skip]
pub fn reverse_related(project: &Project, key: RelationKey, to: Selection) -> Vec<Selection> {
    let Some(row) = relation(key) else { return Vec::new() };
    let sources: Vec<Selection> = match row.from {
        ElementKind::Asset => project.assets().map(|(_, id, _)| Selection::Asset(id)).collect(),
        ElementKind::Threat => project.threat_scenarios().map(|(_, id, _)| Selection::Threat(id)).collect(),
        ElementKind::Goal => project.cybersecurity_goals().map(|(_, id, _)| Selection::Goal(id)).collect(),
        ElementKind::Control => project.security_controls().map(|(_, id, _)| Selection::Control(id)).collect(),
        ElementKind::DataFlow => project.data_flows().map(|(_, id, _)| Selection::DataFlow(id)).collect(),
        ElementKind::Step => project.attack_trees().flat_map(|(_, id, t)| t.iter().map(move |(_, n, _)| Selection::TreeNode(id, n))).collect(),
        _ => Vec::new(),
    };
    sources.into_iter().filter(|s| (row.read)(project, *s).contains(&to)).collect()
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
    threat(project, from).and_then(|t| t.goal()).and_then(|g| project.cybersecurity_goal_id(g)).map(Selection::Goal).into_iter().collect()
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
fn read_control_targets(project: &Project, from: Selection) -> Vec<Selection> {
    let Selection::Control(id) = from else { return Vec::new() };
    project.security_control_idx(&id).map_or_else(Vec::new, |i| project.control_targets(i).filter_map(item_selection).collect())
}
#[rustfmt::skip]
fn read_step_via(project: &Project, from: Selection) -> Vec<Selection> {
    step(project, from).and_then(|(tree, node)| project.step_via(tree, node)).and_then(item_selection).into_iter().collect()
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
    match (from, to) { (Selection::Threat(t), Selection::Goal(g)) => Some(Edit::SetThreatGoal { threat: t, goal: Some(g) }), _ => None }
}
#[rustfmt::skip]
fn unlink_threat_goal(from: Selection, to: Selection) -> Option<Edit> {
    match (from, to) { (Selection::Threat(t), Selection::Goal(_)) => Some(Edit::SetThreatGoal { threat: t, goal: None }), _ => None }
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
fn link_control_targets(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::Control(c), Some(target)) => Some(Edit::ApplyControl { control: c, target }), _ => None }
}
#[rustfmt::skip]
fn unlink_control_targets(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::Control(c), Some(target)) => Some(Edit::UnapplyControl { control: c, target }), _ => None }
}
#[rustfmt::skip]
fn link_step_via(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::TreeNode(tree, node), Some(via)) => Some(Edit::SetStepVia { tree, node, via: Some(via) }), _ => None }
}
#[rustfmt::skip]
fn unlink_step_via(from: Selection, to: Selection) -> Option<Edit> {
    match (from, selection_item(to)) { (Selection::TreeNode(tree, node), Some(_)) => Some(Edit::SetStepVia { tree, node, via: None }), _ => None }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use aegis_model::{
        AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GoalId, InterfaceId,
        NodeId, ThreatId, TreeId, TrustBoundaryId,
    };

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
            Selection::Control(ControlId::new_v7()),
            Selection::Goal(GoalId::new_v7()),
            Selection::Claim(ClaimId::new_v7()),
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
}
