//! Toolkit-independent view model and cause resolution for the right-hand
//! inspector dock panel (WBX-06, WBX-11, UI-15).

use aegis_canvas::Tone;
use aegis_model::{
    CeilingSource, ElementRef, Evaluation, FeasibilityLevel, GateKind, ItemRef, Node, Project,
    Reason, StepCeiling, ThreatId, TreeId, TreeIdx,
};

use crate::i18n::Catalog;
use crate::pages::{self, selection_visual_id};
use crate::relations::RelationKey;
use crate::table::{LinkChip, selection_label};
use crate::view::{Selection, through_label};
use crate::workspace::Workspace;

/// One forward or reverse relation row in the inspector (WBX-06, UI-15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorRelationRow {
    /// Stable relation identifier from [`crate::relations::RELATIONS`].
    pub key: RelationKey,
    /// Localized display label for this relation.
    pub label: String,
    /// Whether this relation can be edited with [`crate::picker::MultiLinkPicker`].
    pub editable: bool,
    /// Linked elements shown as clickable chips.
    pub chips: Vec<LinkChip>,
}

/// One labeled property or computed summary fact in the inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorField {
    /// Localized field label.
    pub label: String,
    /// Display value.
    pub value: String,
    /// Optional risk or feasibility color bucket for badge styling.
    pub tone: Option<Tone>,
}

/// One navigable cause target in an explain-why trace or exclusion banner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CauseTarget {
    /// Localized role of the cause (e.g. "Deciding step", "Capped by", "Excluded by").
    pub role: String,
    /// Short visual ID of the target element (`AT00`, `D01`, `SC00`, `COND00`).
    pub visual_id: String,
    /// Human-readable label of the target element or step.
    pub label: String,
    /// Selection opened when the user activates this cause.
    pub target: Selection,
}

/// Step-by-step explain-why derivation and cause targets for the selection (WBX-06).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorExplain {
    /// Human-readable derivation lines (inherent/residual risk, gate rule, ceilings).
    pub lines: Vec<String>,
    /// Ordered cause targets, primary deciding cause first.
    pub causes: Vec<CauseTarget>,
}

/// Complete toolkit-free view model for the right-hand inspector dock panel (WBX-06).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InspectorView {
    /// The inspected selection.
    pub selection: Selection,
    /// Short human-readable visual ID (`T00`, `AT00`, `C01`, ...).
    pub visual_id: String,
    /// Display name or step label.
    pub name: String,
    /// Localized singular kind label (`Threat scenario`, `Step`, `OR gate`, ...).
    pub kind: String,
    /// Names of the systems this element belongs to or is used in (`project-explorer.md` §4).
    pub systems: Vec<String>,
    /// Human-readable exclusion reason when the selection is excluded by an off condition.
    pub excluded_reason: Option<String>,
    /// Navigable cause targets for why the selection is excluded.
    pub exclusion_causes: Vec<CauseTarget>,
    /// Provenance summary (`Last changed by ...`), empty when unset.
    pub changed: String,
    /// Description text, empty when unset.
    pub description: String,
    /// Summary property and rating fields.
    pub fields: Vec<InspectorField>,
    /// Explain-why derivation trace and cause links, if applicable.
    pub explain: Option<InspectorExplain>,
    /// Primary cause target jumped to by the `"go-to-cause"` command.
    pub primary_cause: Option<Selection>,
    /// Forward relation rows from [`crate::relations::RELATIONS`] for the selection's kind.
    pub relations: Vec<InspectorRelationRow>,
    /// Non-empty reverse trace-spine neighbour rows pointing to the selection.
    pub reverse_relations: Vec<InspectorRelationRow>,
}

/// Localized reverse-relation label for `key` in the inspector (`Used by` section).
#[must_use]
pub fn reverse_relation_label(key: RelationKey, catalog: &Catalog) -> String {
    let id = match key {
        RelationKey::ASSET_COMPONENT => "inspector-rev-asset-component",
        RelationKey::THREAT_ASSET => "inspector-rev-threat-asset",
        RelationKey::THREAT_DAMAGES => "inspector-rev-threat-damages",
        RelationKey::THREAT_TREE => "inspector-rev-threat-tree",
        RelationKey::THREAT_GOAL => "inspector-rev-threat-goal",
        RelationKey::THREAT_CLAIM => "inspector-rev-threat-claim",
        RelationKey::GOAL_CONTROLS => "inspector-rev-goal-controls",
        RelationKey::GOAL_RESPONSIBLE => "inspector-rev-goal-responsible",
        RelationKey::CLAIM_RESPONSIBLE => "inspector-rev-claim-responsible",
        RelationKey::CONTROL_TARGETS => "inspector-rev-control-targets",
        RelationKey::STEP_VIA => "inspector-rev-step-via",
        RelationKey::STEP_MITIGATIONS => "inspector-rev-step-mitigations",
        RelationKey::DATAFLOW_FROM => "inspector-rev-dataflow-from",
        RelationKey::DATAFLOW_TO => "inspector-rev-dataflow-to",
        RelationKey::DATAFLOW_INTERFACE => "inspector-rev-dataflow-interface",
        RelationKey::INTERFACE_COMPONENT => "inspector-rev-interface-component",
        RelationKey::BOUNDARY_MEMBERS => "inspector-rev-boundary-members",
        _ => "inspector-rev-conditions",
    };
    catalog.tr(id)
}

fn reason_target(reason: Reason) -> Option<Selection> {
    match reason {
        Reason::ConditionOff(cid) => Some(Selection::Condition(cid)),
        Reason::OwnerExcluded(owner) => match owner {
            ElementRef::Asset(id) => Some(Selection::Asset(id)),
            ElementRef::Damage(id) => Some(Selection::Damage(id)),
            ElementRef::Threat(id) => Some(Selection::Threat(id)),
            ElementRef::Tree(id) => Some(Selection::Tree(id)),
            ElementRef::Control(id) => Some(Selection::Control(id)),
            ElementRef::Goal(id) => Some(Selection::Goal(id)),
            ElementRef::Claim(id) => Some(Selection::Claim(id)),
            ElementRef::Component(id) => Some(Selection::Component(id)),
            ElementRef::Interface(id) => Some(Selection::Interface(id)),
            ElementRef::DataFlow(id) => Some(Selection::DataFlow(id)),
            ElementRef::TrustBoundary(id) => Some(Selection::TrustBoundary(id)),
            _ => None,
        },
        Reason::ViaExcluded(ItemRef::Component(cid)) => Some(Selection::Component(cid)),
        Reason::ViaExcluded(ItemRef::Interface(iid)) => Some(Selection::Interface(iid)),
        Reason::AllPathsExcluded | _ => None,
    }
}

fn exclusion_targets(project: &Project, sel: Selection) -> Vec<Selection> {
    let app = project.applicability();
    let reasons = match sel {
        Selection::TreeNode(tree, node) => app.node_reasons(tree, node),
        Selection::Party(_) | Selection::Condition(_) => return Vec::new(),
        Selection::Asset(id) => app.reasons(ElementRef::Asset(id)),
        Selection::Damage(id) => app.reasons(ElementRef::Damage(id)),
        Selection::Threat(id) => app.reasons(ElementRef::Threat(id)),
        Selection::Tree(id) => app.reasons(ElementRef::Tree(id)),
        Selection::Control(id) => app.reasons(ElementRef::Control(id)),
        Selection::Goal(id) => app.reasons(ElementRef::Goal(id)),
        Selection::Claim(id) => app.reasons(ElementRef::Claim(id)),
        Selection::Component(id) => app.reasons(ElementRef::Component(id)),
        Selection::Interface(id) => app.reasons(ElementRef::Interface(id)),
        Selection::DataFlow(id) => app.reasons(ElementRef::DataFlow(id)),
        Selection::TrustBoundary(id) => app.reasons(ElementRef::TrustBoundary(id)),
    };
    reasons.iter().copied().filter_map(reason_target).collect()
}

/// Navigable cause targets for why `sel` is excluded by an off condition.
#[must_use]
pub fn exclusion_causes(project: &Project, sel: Selection, catalog: &Catalog) -> Vec<CauseTarget> {
    let role = catalog.tr("inspector-excluded-by");
    exclusion_targets(project, sel)
        .into_iter()
        .map(|target| CauseTarget {
            role: role.clone(),
            visual_id: selection_visual_id(project, target),
            label: selection_label(project, target),
            target,
        })
        .collect()
}

fn make_cause(
    project: &Project,
    catalog: Option<&Catalog>,
    role_key: &str,
    target: Selection,
) -> CauseTarget {
    CauseTarget {
        role: catalog.map_or_else(String::new, |c| c.tr(role_key)),
        visual_id: selection_visual_id(project, target),
        label: selection_label(project, target),
        target,
    }
}

fn push_ceiling_causes(
    project: &Project,
    catalog: Option<&Catalog>,
    ceilings: &[StepCeiling],
    causes: &mut Vec<CauseTarget>,
) {
    for c in ceilings {
        let Some(cid) = project.security_control_id(c.control()) else {
            continue;
        };
        let target = Selection::Control(cid);
        if !causes.iter().any(|existing| existing.target == target) {
            causes.push(make_cause(project, catalog, "inspector-capped-by", target));
        }
    }
}

fn push_tree_deciding_step(
    project: &Project,
    catalog: Option<&Catalog>,
    tree_id: TreeId,
    tree_idx: TreeIdx,
    residual_eval: Option<&Evaluation<FeasibilityLevel>>,
    inherent_eval: Option<&Evaluation<FeasibilityLevel>>,
    causes: &mut Vec<CauseTarget>,
) {
    let Some(tree) = project.attack_tree(tree_idx) else {
        return;
    };
    if let Some(eval) = residual_eval
        && let Some(step) = project.deciding_step(tree_idx, eval)
    {
        let step_node_id = step.node_id();
        let label = tree
            .idx_of(&step_node_id)
            .and_then(|ni| tree.get(ni))
            .map_or("", Node::label)
            .to_owned();
        causes.push(CauseTarget {
            role: catalog.map_or_else(String::new, |c| c.tr("inspector-deciding-step")),
            visual_id: project.attack_tree_visual_id(tree_idx).unwrap_or_default(),
            label,
            target: Selection::TreeNode(tree_id, step_node_id),
        });
        push_ceiling_causes(project, catalog, step.ceilings(), causes);
        return;
    }
    if let Some(eval) = residual_eval.or(inherent_eval)
        && let Some((ni, node)) = eval
            .why_path(tree.root())
            .filter_map(|ni| Some((ni, tree.get(ni)?)))
            .filter(|(_, n)| n.gate_kind().is_none())
            .last()
        && let Some(nid) = tree.id_of(ni)
    {
        causes.push(CauseTarget {
            role: catalog.map_or_else(String::new, |c| c.tr("inspector-deciding-step")),
            visual_id: project.attack_tree_visual_id(tree_idx).unwrap_or_default(),
            label: node.label().to_owned(),
            target: Selection::TreeNode(tree_id, nid),
        });
    }
}

fn explain_threat(
    ws: &Workspace,
    id: ThreatId,
    catalog: Option<&Catalog>,
) -> Option<InspectorExplain> {
    let project = ws.project();
    let t_idx = project.threat_scenario_idx(&id)?;
    let t = project.threat_scenario(t_idx)?;
    let lines = if catalog.is_some() {
        pages::threat_detail(ws, id)?.why
    } else {
        Vec::new()
    };
    let mut causes = Vec::new();
    if let Some(tree_idx) = t.attack_tree()
        && let Some(tree_id) = project.attack_tree_id(tree_idx)
    {
        let res = ws.cache().evaluate_residual_threat(project, id).ok();
        let inh = ws.cache().evaluate_threat(project, id).ok();
        push_tree_deciding_step(project, catalog, tree_id, tree_idx, res, inh, &mut causes);
    }
    let deciding_damage = ws
        .cache()
        .evaluate_residual_risk(project, id)
        .ok()
        .and_then(|r| r.deciding_entry())
        .or_else(|| {
            ws.cache()
                .evaluate_risk(project, id)
                .ok()
                .and_then(|r| r.deciding_entry())
        });
    if let Some(entry) = deciding_damage {
        causes.push(make_cause(
            project,
            catalog,
            "inspector-deciding-damage",
            Selection::Damage(entry.damage()),
        ));
    }
    Some(InspectorExplain { lines, causes })
}

fn explain_tree(
    ws: &Workspace,
    tree_id: TreeId,
    catalog: Option<&Catalog>,
) -> Option<InspectorExplain> {
    let project = ws.project();
    let m = project.methodology();
    let tree_idx = project.attack_tree_idx(&tree_id)?;
    let res = ws.cache().evaluate_residual_tree(project, tree_id).ok();
    let inh = ws.cache().evaluate_tree(project, tree_id).ok();
    res.or(inh)?;
    let mut lines = Vec::new();
    if let Some(e) = inh {
        let lvl = m.feasibility_level_label(*e.root_value()).unwrap_or("?");
        lines.push(format!("Feasibility before controls: {lvl}"));
    }
    if let Some(e) = res {
        let lvl = m.feasibility_level_label(*e.root_value()).unwrap_or("?");
        lines.push(format!("Feasibility after controls: {lvl}"));
    }
    let mut causes = Vec::new();
    push_tree_deciding_step(project, catalog, tree_id, tree_idx, res, inh, &mut causes);
    (!lines.is_empty() || !causes.is_empty()).then_some(InspectorExplain { lines, causes })
}

fn explain_tree_node(
    ws: &Workspace,
    tree_id: TreeId,
    node_id: aegis_model::NodeId,
    catalog: Option<&Catalog>,
) -> Option<InspectorExplain> {
    let project = ws.project();
    let m = project.methodology();
    let tree_idx = project.attack_tree_idx(&tree_id)?;
    let tree = project.attack_tree(tree_idx)?;
    let node_idx = tree.idx_of(&node_id)?;
    let node = tree.get(node_idx)?;
    let mut lines = Vec::new();
    let mut causes = Vec::new();
    if node.gate_kind().is_some() {
        let eval = ws
            .cache()
            .evaluate_residual_tree(project, tree_id)
            .or_else(|_| ws.cache().evaluate_tree(project, tree_id))
            .ok()?;
        let explanation = eval.explain(tree, node_idx)?;
        let rule = match explanation.rule() {
            Some(GateKind::And) => "AND: the hardest child decides",
            Some(GateKind::Or) => "OR: the easiest child decides",
            Some(GateKind::SeqAnd) => "Sequential AND: the hardest step decides",
            None => "",
        };
        if !rule.is_empty() {
            lines.push(rule.to_owned());
        }
        if let Some(child_idx) = explanation.decisive()
            && let (Some(child), Some(child_id)) = (tree.get(child_idx), tree.id_of(child_idx))
        {
            let lvl = eval
                .get(child_idx)
                .and_then(|v| m.feasibility_level_label(*v))
                .unwrap_or("unrated");
            lines.push(format!("Decided by \"{}\" ({lvl})", child.label()));
            let vis = project.attack_tree_visual_id(tree_idx).unwrap_or_default();
            causes.push(CauseTarget {
                role: catalog.map_or_else(String::new, |c| c.tr("inspector-deciding-child")),
                visual_id: vis.clone(),
                label: child.label().to_owned(),
                target: Selection::TreeNode(tree_id, child_id),
            });
            if child.gate_kind().is_some()
                && let Some((leaf_idx, leaf)) = eval
                    .why_path(child_idx)
                    .filter_map(|ni| Some((ni, tree.get(ni)?)))
                    .filter(|(_, n)| n.gate_kind().is_none())
                    .last()
                && let Some(leaf_id) = tree.id_of(leaf_idx)
            {
                causes.push(CauseTarget {
                    role: catalog.map_or_else(String::new, |c| c.tr("inspector-deciding-step")),
                    visual_id: vis,
                    label: leaf.label().to_owned(),
                    target: Selection::TreeNode(tree_id, leaf_id),
                });
            }
        }
    } else {
        let ceilings = project.step_ceilings(tree_idx, node_idx);
        for c in &ceilings {
            let Some(ctrl) = project.security_control(c.control()) else {
                continue;
            };
            let name = ctrl.name();
            let lvl = m.feasibility_level_label(c.feasibility()).unwrap_or("?");
            let line = match c.source() {
                CeilingSource::ControlRule { target, .. } => {
                    format!("{name} (from {}): {lvl}", through_label(project, target))
                }
                CeilingSource::StepMitigation => format!("{name} (on this step): {lvl}"),
            };
            lines.push(line);
        }
        push_ceiling_causes(project, catalog, &ceilings, &mut causes);
        if let Some(via) = project.step_via(tree_idx, node_idx) {
            let target = match via {
                ItemRef::Component(cid) => Selection::Component(cid),
                ItemRef::Interface(iid) => Selection::Interface(iid),
            };
            causes.push(make_cause(project, catalog, "through", target));
        }
    }
    (!lines.is_empty() || !causes.is_empty()).then_some(InspectorExplain { lines, causes })
}

fn explain_inner(
    ws: &Workspace,
    sel: Selection,
    catalog: Option<&Catalog>,
) -> Option<InspectorExplain> {
    match sel {
        Selection::Threat(id) => explain_threat(ws, id, catalog),
        Selection::Tree(id) => explain_tree(ws, id, catalog),
        Selection::TreeNode(tree_id, node_id) => explain_tree_node(ws, tree_id, node_id, catalog),
        _ => None,
    }
}

/// Step-by-step explain-why derivation and cause targets for `sel` (WBX-06).
#[must_use]
pub fn explain_selection(
    ws: &Workspace,
    sel: Selection,
    catalog: &Catalog,
) -> Option<InspectorExplain> {
    explain_inner(ws, sel, Some(catalog))
}

/// Primary deciding cause for `sel`, jumped to by `"go-to-cause"` (WBX-06, WBX-11).
#[must_use]
pub fn primary_cause(ws: &Workspace, sel: Selection) -> Option<Selection> {
    if let Some(explain) = explain_inner(ws, sel, None)
        && let Some(first) = explain.causes.first()
    {
        return Some(first.target);
    }
    exclusion_targets(ws.project(), sel).into_iter().next()
}
