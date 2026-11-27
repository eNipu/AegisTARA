//! Toolkit-independent view model and cause resolution for the right-hand
//! inspector dock panel (WBX-06, WBX-11, UI-15).

use aegis_canvas::Tone;
use aegis_model::{
    CeilingSource, ElementRef, Evaluation, FeasibilityLevel, GateKind, ItemRef, Methodology, Node,
    PartyKind, Project, Reason, StepCeiling, StepTarget, ThreatId, TreeId, TreeIdx,
};

use crate::commands::step_op_target;
use crate::i18n::Catalog;
use crate::pages::{self, selection_visual_id};
use crate::relations::{ElementKind, RELATIONS, RelationKey};
use crate::table::{
    LinkChip, selection_element_ref, selection_exclusion, selection_label, selection_system_set,
};
use crate::view::{Selection, format_shared_step_line, steps_through, through_label};
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
    /// Shared-step usage line for a linked tree node.
    pub shared_note: Option<String>,
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
        RelationKey::STEP_VIA | RelationKey::SHARED_STEP_VIA => "inspector-rev-step-via",
        RelationKey::FRAGMENT_USED_IN => "inspector-rev-fragment-used-in",
        RelationKey::SHARED_STEP_USED_IN => "inspector-rev-shared-step-used-in",
        RelationKey::STEP_MITIGATIONS => "inspector-rev-step-mitigations",
        RelationKey::DATAFLOW_FROM => "inspector-rev-dataflow-from",
        RelationKey::DATAFLOW_TO => "inspector-rev-dataflow-to",
        RelationKey::DATAFLOW_INTERFACE => "inspector-rev-dataflow-interface",
        RelationKey::INTERFACE_COMPONENT => "inspector-rev-interface-component",
        RelationKey::BOUNDARY_MEMBERS => "inspector-rev-boundary-members",
        RelationKey::CLAIM_ASSUMPTIONS => "inspector-rev-claim-assumptions",
        RelationKey::ASSUMPTION_EXCLUDES => "inspector-rev-assumption-excludes",
        _ => "inspector-rev-conditions",
    };
    catalog.tr(id)
}

pub(crate) fn reason_target(reason: Reason) -> Option<Selection> {
    match reason {
        Reason::ConditionOff(cid) => Some(Selection::Condition(cid)),
        Reason::AssumptionHolds(aid) => Some(Selection::Assumption(aid)),
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
            ElementRef::Assumption(id) => Some(Selection::Assumption(id)),
            ElementRef::SharedStep(_) => None,
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
        Selection::SharedStep(_) | Selection::Party(_) | Selection::Condition(_) => {
            return Vec::new();
        }
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
        Selection::Assumption(id) => app.reasons(ElementRef::Assumption(id)),
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

fn push_path_node(
    project: &Project,
    catalog: Option<&Catalog>,
    tree_id: TreeId,
    node_id: aegis_model::NodeId,
    causes: &mut Vec<CauseTarget>,
) -> Option<(TreeIdx, aegis_model::NodeIdx)> {
    let tree_idx = project.attack_tree_idx(&tree_id)?;
    let tree = project.attack_tree(tree_idx)?;
    let node_idx = tree.idx_of(&node_id)?;
    let node = tree.get(node_idx)?;
    let target = Selection::TreeNode(tree_id, node_id);
    if causes.iter().any(|cause| cause.target == target) {
        return Some((tree_idx, node_idx));
    }
    let role = if project.step_fragment_id(tree_idx, node_idx).is_some() {
        "inspector-fragment-reference"
    } else {
        "inspector-deciding-step"
    };
    causes.push(CauseTarget {
        role: catalog.map_or_else(String::new, |c| c.tr(role)),
        visual_id: project.attack_tree_visual_id(tree_idx).unwrap_or_default(),
        label: node.label().to_owned(),
        target,
    });
    Some((tree_idx, node_idx))
}

fn push_condition_causes(
    project: &Project,
    catalog: Option<&Catalog>,
    selection: Selection,
    causes: &mut Vec<CauseTarget>,
) {
    for target in exclusion_targets(project, selection) {
        if !causes.iter().any(|cause| cause.target == target) {
            causes.push(make_cause(
                project,
                catalog,
                "inspector-excluded-by",
                target,
            ));
        }
    }
}

fn cached_node_value(
    ws: &Workspace,
    tree_id: TreeId,
    node_idx: aegis_model::NodeIdx,
    residual: bool,
) -> Option<FeasibilityLevel> {
    let project = ws.project();
    let eval = if residual {
        ws.cache().evaluate_residual_tree(project, tree_id)
    } else {
        ws.cache().evaluate_tree(project, tree_id)
    };
    eval.ok()?.get(node_idx).copied()
}

fn push_fragment_deciding_path(
    ws: &Workspace,
    catalog: Option<&Catalog>,
    tree_id: TreeId,
    node_id: aegis_model::NodeId,
    residual: bool,
    causes: &mut Vec<CauseTarget>,
) {
    let project = ws.project();
    let Ok(path) = project.fragment_why_path(tree_id, node_id, residual) else {
        return;
    };
    let crosses_tree = path.windows(2).any(|pair| pair[0].tree != pair[1].tree);
    if !crosses_tree {
        for step in path.iter().rev() {
            let Some(tree_idx) = project.attack_tree_idx(&step.tree) else {
                continue;
            };
            let Some(tree) = project.attack_tree(tree_idx) else {
                continue;
            };
            let Some(node_idx) = tree.idx_of(&step.node) else {
                continue;
            };
            if tree
                .get(node_idx)
                .is_some_and(|node| node.gate_kind().is_none())
            {
                push_path_node(project, catalog, step.tree, step.node, causes);
                break;
            }
        }
        return;
    }

    for (position, step) in path.iter().enumerate() {
        let boundary_host = path
            .get(position + 1)
            .is_some_and(|next| next.tree != step.tree);
        let boundary_target = position > 0 && path[position - 1].tree != step.tree;
        if !boundary_host && !boundary_target && position + 1 != path.len() {
            continue;
        }
        let Some((tree_idx, node_idx)) =
            push_path_node(project, catalog, step.tree, step.node, causes)
        else {
            continue;
        };
        if !boundary_host {
            continue;
        }
        let target_root = path[position + 1];
        let target_idx = project.attack_tree_idx(&target_root.tree);
        let target_value = target_idx.and_then(|idx| {
            cached_node_value(
                ws,
                target_root.tree,
                project.attack_tree(idx)?.root(),
                residual,
            )
        });
        let host_value = cached_node_value(ws, step.tree, node_idx, residual);
        if host_value
            .zip(target_value)
            .is_some_and(|(host, target)| host != target)
        {
            push_ceiling_causes(
                project,
                catalog,
                &project.step_ceilings(tree_idx, node_idx),
                causes,
            );
            push_condition_causes(
                project,
                catalog,
                Selection::TreeNode(step.tree, step.node),
                causes,
            );
        }
    }
}

fn push_tree_deciding_step(
    ws: &Workspace,
    catalog: Option<&Catalog>,
    tree_id: TreeId,
    tree_idx: TreeIdx,
    residual_eval: Option<&Evaluation<FeasibilityLevel>>,
    inherent_eval: Option<&Evaluation<FeasibilityLevel>>,
    causes: &mut Vec<CauseTarget>,
) {
    let project = ws.project();
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
    if residual_eval.is_none() && inherent_eval.is_none() {
        return;
    }
    if let Some(root_id) = tree.id_of(tree.root()) {
        push_fragment_deciding_path(
            ws,
            catalog,
            tree_id,
            root_id,
            residual_eval.is_some(),
            causes,
        );
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
        push_tree_deciding_step(ws, catalog, tree_id, tree_idx, res, inh, &mut causes);
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
    push_tree_deciding_step(ws, catalog, tree_id, tree_idx, res, inh, &mut causes);
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
        let residual_eval = ws.cache().evaluate_residual_tree(project, tree_id);
        let (eval, residual) = match residual_eval {
            Ok(eval) => (eval, true),
            Err(_) => (ws.cache().evaluate_tree(project, tree_id).ok()?, false),
        };
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
                visual_id: vis,
                label: child.label().to_owned(),
                target: Selection::TreeNode(tree_id, child_id),
            });
        }
        push_fragment_deciding_path(ws, catalog, tree_id, node_id, residual, &mut causes);
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
        if let Some(fragment) = project.step_fragment_id(tree_idx, node_idx) {
            push_condition_causes(
                project,
                catalog,
                Selection::TreeNode(tree_id, node_id),
                &mut causes,
            );
            let residual = ws.cache().evaluate_residual_tree(project, tree_id).is_ok();
            if residual || ws.cache().evaluate_tree(project, tree_id).is_ok() {
                push_fragment_deciding_path(ws, catalog, tree_id, node_id, residual, &mut causes);
            }
            push_condition_causes(project, catalog, Selection::Tree(fragment), &mut causes);
        }
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

fn feasibility_tone(m: &Methodology, level: FeasibilityLevel) -> Tone {
    let index = m
        .feasibility_levels()
        .position(|(l, _, _)| l == level)
        .unwrap_or(0);
    Tone::from_level(index, m.feasibility_levels().len())
}

fn feasibility_text(m: &Methodology, lang: &str, level: FeasibilityLevel) -> String {
    m.feasibility_level_label_for_lang(level, lang)
        .or_else(|| m.feasibility_level_label(level))
        .unwrap_or("?")
        .to_owned()
}

fn field(label: impl Into<String>, value: impl Into<String>, tone: Option<Tone>) -> InspectorField {
    InspectorField {
        label: label.into(),
        value: value.into(),
        tone,
    }
}

fn to_chips(project: &Project, targets: Vec<Selection>) -> Vec<LinkChip> {
    targets
        .into_iter()
        .map(|target| LinkChip {
            target,
            visual_id: selection_visual_id(project, target),
            label: selection_label(project, target),
        })
        .collect()
}

/// Complete toolkit-independent inspector view model for `sel` (WBX-06, UI-15).
///
/// Returns `None` if `sel` does not exist in `ws.project()`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn inspector_view(ws: &Workspace, sel: Selection, catalog: &Catalog) -> Option<InspectorView> {
    let project = ws.project();
    let m = project.methodology();
    let lang = catalog.language().code();
    let vocab = pages::vocabulary_for_lang(m, lang);
    let names = &vocab.element_names;
    let unrated = || catalog.tr("not-rated");
    let mut fields = Vec::new();

    let (kind, changed) = match sel {
        Selection::Component(id) => {
            let d = pages::component_detail(ws, id)?;
            let sys = if !d.system_from_parent.is_empty() {
                d.system_from_parent
            } else {
                d.system
                    .and_then(|s| crate::systems::system_name(project, s))
                    .unwrap_or_default()
            };
            if !sys.is_empty() {
                fields.push(field(catalog.tr("report-systems"), sys, None));
            }
            (names.component.singular.to_string(), d.changed)
        }
        Selection::Interface(id) => {
            let d = pages::interface_detail(ws, id)?;
            if let Some(k) = vocab.interface_kinds.get(d.kind) {
                fields.push(field(catalog.tr("kind"), k.clone(), None));
            }
            (names.interface.singular.to_string(), d.changed)
        }
        Selection::DataFlow(id) => {
            let d = pages::data_flow_detail(ws, id)?;
            if let Some(b) = d.boundary {
                fields.push(field(names.trust_boundary.singular.to_string(), b, None));
            } else if d.crosses_boundary {
                fields.push(field(
                    names.trust_boundary.singular.to_string(),
                    catalog.tr("yes"),
                    None,
                ));
            }
            (names.data_flow.singular.to_string(), d.changed)
        }
        Selection::TrustBoundary(id) => {
            let d = pages::trust_boundary_detail(ws, id)?;
            (names.trust_boundary.singular.to_string(), d.changed)
        }
        Selection::Asset(id) => {
            let a = project.asset(project.asset_idx(&id)?)?;
            let props = m
                .properties()
                .zip(&vocab.properties)
                .filter(|(p, _)| a.properties().contains(*p))
                .map(|(_, l)| l.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            fields.push(field(catalog.tr("properties"), props, None));
            (
                names.asset.singular.to_string(),
                pages::changed_text(a.provenance()),
            )
        }
        Selection::Damage(id) => {
            let d = project.damage_scenario(project.damage_scenario_idx(&id)?)?;
            for (cat, cv) in m.impact_categories().zip(&vocab.impact_categories) {
                if let Some(lvl) = d.impact(cat)
                    && let Some(pos) = m.levels(cat).position(|x| x == lvl)
                    && let Some(lbl) = cv.levels.get(pos)
                {
                    fields.push(field(cv.label.clone(), lbl.clone(), None));
                }
            }
            if fields.is_empty() {
                fields.push(field(catalog.tr("impact"), unrated(), None));
            }
            if !d.impact_rationale().is_empty() {
                fields.push(field(catalog.tr("rationale"), d.impact_rationale(), None));
            }
            (
                names.damage_scenario.singular.to_string(),
                pages::changed_text(d.provenance()),
            )
        }
        Selection::Threat(id) => {
            let d = pages::threat_detail(ws, id)?;
            fields.push(field(
                catalog.tr("risk-before-controls"),
                d.inherent
                    .as_ref()
                    .map_or_else(unrated, |r| r.label.clone()),
                d.inherent.as_ref().map(|r| r.tone),
            ));
            fields.push(field(
                catalog.tr("risk-after-controls"),
                d.residual
                    .as_ref()
                    .map_or_else(unrated, |r| r.label.clone()),
                d.residual.as_ref().map(|r| r.tone),
            ));
            if let Some(cat) = vocab.threat_categories.get(d.category) {
                fields.push(field(catalog.tr("category"), cat.clone(), None));
            }
            fields.push(field(
                catalog.tr("feasibility"),
                d.residual_feasibility
                    .or(d.feasibility)
                    .unwrap_or_else(unrated),
                None,
            ));
            if let Ok(risk) = ws.cache().evaluate_risk(project, id)
                && let Some(entry) = risk.deciding_entry()
            {
                let dmg = selection_label(project, Selection::Damage(entry.damage()));
                let cat = m
                    .impact_category_label_for_lang(entry.category(), lang)
                    .unwrap_or("?");
                let lvl = m
                    .level_label_for_lang(entry.category(), entry.impact(), lang)
                    .unwrap_or("?");
                fields.push(field(
                    catalog.tr("deciding-damage"),
                    format!("{dmg} ({cat}: {lvl})"),
                    None,
                ));
            }
            if let Some(pos) = d.treatment
                && let Some(tr) = vocab.treatments.get(pos)
            {
                fields.push(field(catalog.tr("treatment"), tr.clone(), None));
            }
            if let Some(scales) = d.scales {
                let yn = catalog.tr(if scales { "yes" } else { "no" });
                let val = if d.scaling_reason.is_empty() {
                    yn
                } else {
                    format!("{yn} ({})", d.scaling_reason)
                };
                fields.push(field(catalog.tr("scales"), val, None));
            }
            if !d.catalog_refs.is_empty() {
                fields.push(field(
                    catalog.tr("catalog-references"),
                    d.catalog_refs.join(", "),
                    None,
                ));
            }
            if !d.rationale.is_empty() {
                fields.push(field(catalog.tr("pair-rationale"), d.rationale, None));
            }
            if !d.feasibility_rationale.is_empty() {
                fields.push(field(
                    catalog.tr("feasibility-rationale"),
                    d.feasibility_rationale,
                    None,
                ));
            }
            (names.threat_scenario.singular.to_string(), d.changed)
        }
        Selection::Tree(id) => {
            let idx = project.attack_tree_idx(&id)?;
            let inh = ws
                .cache()
                .evaluate_tree(project, id)
                .ok()
                .map(|e| *e.root_value());
            let res = ws
                .cache()
                .evaluate_residual_tree(project, id)
                .ok()
                .map(|e| *e.root_value());
            for lvl in [inh, res].into_iter().flatten() {
                fields.push(field(
                    catalog.tr("feasibility"),
                    feasibility_text(m, lang, lvl),
                    Some(feasibility_tone(m, lvl)),
                ));
            }
            if fields.is_empty() {
                fields.push(field(catalog.tr("feasibility"), unrated(), None));
            }
            let changed = project
                .attack_tree_provenance(idx)
                .map(pages::changed_text)
                .unwrap_or_default();
            (names.attack_tree.singular.to_string(), changed)
        }
        Selection::TreeNode(tree_id, node_id) => {
            let t_idx = project.attack_tree_idx(&tree_id)?;
            let tree = project.attack_tree(t_idx)?;
            let n_idx = tree.idx_of(&node_id)?;
            let node = tree.get(n_idx)?;
            let fragment = project.step_fragment_id(t_idx, n_idx);
            let kind = if fragment.is_some() {
                catalog.tr("inspector-fragment-reference")
            } else {
                match node.gate_kind() {
                    None => "Step",
                    Some(GateKind::Or) => "OR gate",
                    Some(GateKind::And) => "AND gate",
                    Some(GateKind::SeqAnd) => "Sequential AND gate",
                }
                .to_owned()
            };
            if node.gate_kind().is_some() {
                let eval = ws
                    .cache()
                    .evaluate_residual_tree(project, tree_id)
                    .or_else(|_| ws.cache().evaluate_tree(project, tree_id))
                    .ok();
                if let Some(lvl) = eval.and_then(|e| e.get(n_idx).copied()) {
                    fields.push(field(
                        catalog.tr("feasibility"),
                        feasibility_text(m, lang, lvl),
                        Some(feasibility_tone(m, lvl)),
                    ));
                } else {
                    fields.push(field(catalog.tr("feasibility"), unrated(), None));
                }
            } else {
                if fragment.is_some() {
                    let inherent = ws
                        .cache()
                        .evaluate_tree(project, tree_id)
                        .ok()
                        .and_then(|eval| eval.get(n_idx).copied());
                    let residual = ws
                        .cache()
                        .evaluate_residual_tree(project, tree_id)
                        .ok()
                        .and_then(|eval| eval.get(n_idx).copied());
                    for (label, value) in [
                        (catalog.tr("before-controls"), inherent),
                        (catalog.tr("after-controls"), residual),
                    ] {
                        if let Some(level) = value {
                            fields.push(field(
                                label,
                                feasibility_text(m, lang, level),
                                Some(feasibility_tone(m, level)),
                            ));
                        } else {
                            fields.push(field(label, unrated(), None));
                        }
                    }
                } else {
                    if let Some(inh) = project.effective_step_feasibility(t_idx, n_idx) {
                        fields.push(field(
                            catalog.tr("before-controls"),
                            feasibility_text(m, lang, inh),
                            Some(feasibility_tone(m, inh)),
                        ));
                    } else {
                        fields.push(field(catalog.tr("before-controls"), unrated(), None));
                    }
                    if let Some(res) = project.residual_step_feasibility(t_idx, n_idx) {
                        fields.push(field(
                            catalog.tr("after-controls"),
                            feasibility_text(m, lang, res),
                            Some(feasibility_tone(m, res)),
                        ));
                    }
                }
                if let Some(p) = project.step_attack_potential(t_idx, n_idx) {
                    let lvl = feasibility_text(m, lang, p.feasibility());
                    fields.push(field(
                        catalog.tr("feasibility"),
                        format!("{} ({lvl})", p.score()),
                        Some(feasibility_tone(m, p.feasibility())),
                    ));
                }
                if let Some(at) = project.step_attack_type(t_idx, n_idx) {
                    let lbl = m
                        .attack_type_label_for_lang(at, lang)
                        .or_else(|| m.attack_type_label(at))
                        .unwrap_or("?");
                    fields.push(field(catalog.tr("attack-type"), lbl, None));
                }
                for f in m.feasibility_factors() {
                    if let Some(lvl) = project.step_factor(t_idx, n_idx, f) {
                        let f_lbl = m.feasibility_factor_label(f).unwrap_or("?");
                        let l_lbl = m.factor_level_label(f, lvl).unwrap_or("?");
                        let pts = m.factor_level_points(f, lvl).unwrap_or(0);
                        fields.push(field(f_lbl, format!("{l_lbl} ({pts})"), None));
                    }
                }
            }
            let changed = project
                .attack_tree_provenance(t_idx)
                .map(pages::changed_text)
                .unwrap_or_default();
            (kind, changed)
        }
        Selection::SharedStep(id) => {
            let idx = project.shared_step_idx(&id)?;
            let shared = project.shared_step(idx)?;
            if let Some(level) = shared.feasibility() {
                fields.push(field(
                    catalog.tr("feasibility"),
                    feasibility_text(m, lang, level),
                    Some(feasibility_tone(m, level)),
                ));
            }
            for factor in m.feasibility_factors() {
                if let Some(level) = shared.factor(factor) {
                    let label = m.feasibility_factor_label(factor).unwrap_or("?");
                    let level_label = m.factor_level_label(factor, level).unwrap_or("?");
                    let points = m.factor_level_points(factor, level).unwrap_or(0);
                    fields.push(field(label, format!("{level_label} ({points})"), None));
                }
            }
            if let Some(via) = project.shared_step_via(idx) {
                fields.push(field(
                    catalog.tr("through"),
                    through_label(project, via),
                    None,
                ));
            }
            if let Some(attack_type) = shared.attack_type() {
                let label = m
                    .attack_type_label_for_lang(attack_type, lang)
                    .or_else(|| m.attack_type_label(attack_type))
                    .unwrap_or("?");
                fields.push(field(catalog.tr("attack-type"), label, None));
            }
            (
                catalog.tr("inspector-rev-shared-step-used-in"),
                pages::changed_text(shared.provenance()),
            )
        }
        Selection::Control(id) => {
            let c = project.security_control(project.security_control_idx(&id)?)?;
            if let Some(k) = c.kind()
                && let Some(lbl) = m
                    .control_kind_label_for_lang(k, lang)
                    .or_else(|| m.control_kind_label(k))
            {
                fields.push(field(catalog.tr("setting"), lbl, None));
            }
            if !c.rules().is_empty() {
                let rules = c
                    .rules()
                    .iter()
                    .map(|r| {
                        format!(
                            "{} \u{2192} {}",
                            m.attack_type_label_for_lang(r.attack_type(), lang)
                                .unwrap_or("?"),
                            feasibility_text(m, lang, r.ceiling())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                fields.push(field(catalog.tr("rules"), rules, None));
            }
            (
                names.security_control.singular.to_string(),
                pages::changed_text(c.provenance()),
            )
        }
        Selection::Goal(id) => {
            let g = project.cybersecurity_goal(project.cybersecurity_goal_idx(&id)?)?;
            (
                names.cybersecurity_goal.singular.to_string(),
                pages::changed_text(g.provenance()),
            )
        }
        Selection::Claim(id) => {
            let c = project.cybersecurity_claim(project.cybersecurity_claim_idx(&id)?)?;
            (
                names.cybersecurity_claim.singular.to_string(),
                pages::changed_text(c.provenance()),
            )
        }
        Selection::Party(id) => {
            let p = project.party(project.party_idx(&id)?)?;
            let role_key = match p.kind() {
                PartyKind::Us => "party-us",
                PartyKind::Customer => "party-customer",
                PartyKind::Supplier => "party-supplier",
                PartyKind::Other => "party-other",
            };
            fields.push(field(catalog.tr("kind"), catalog.tr(role_key), None));
            (
                catalog.tr("responsible"),
                pages::changed_text(p.provenance()),
            )
        }
        Selection::Condition(id) => {
            let c = project.condition(project.condition_idx(&id)?)?;
            fields.push(field(
                catalog.tr("condition-present"),
                catalog.tr(if c.present() { "yes" } else { "no" }),
                None,
            ));
            (catalog.tr("condition"), pages::changed_text(c.provenance()))
        }
        Selection::Assumption(id) => {
            let a = project.assumption(project.assumption_idx(&id)?)?;
            fields.push(field(
                catalog.tr("assumption-holds"),
                catalog.tr(if a.holds() { "yes" } else { "no" }),
                None,
            ));
            (
                catalog.tr("assumption"),
                pages::changed_text(a.provenance()),
            )
        }
    };

    let systems = if project.element_count(aegis_model::ElementKind::System) > 0 {
        let usage = project.system_use();
        selection_system_set(project, &usage, sel)
            .map(|set| {
                usage
                    .ids(set)
                    .filter_map(|id| crate::systems::system_name(project, id))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let description = match sel {
        Selection::Condition(id) => project
            .condition_idx(&id)
            .and_then(|i| project.condition(i))
            .map_or_else(String::new, |c| c.description().to_owned()),
        _ => selection_element_ref(sel)
            .and_then(|e| project.description(e))
            .unwrap_or_default()
            .to_owned(),
    };

    let elem_kind = ElementKind::kind_of(sel);
    let shared_note = match sel {
        Selection::TreeNode(tree, node) => match step_op_target(project, tree, node) {
            StepTarget::Shared(id) => project
                .shared_step_idx(&id)
                .and_then(|shared| format_shared_step_line(project, shared, catalog)),
            _ => (|| {
                let t = project.attack_tree_idx(&tree)?;
                let n = project.attack_tree(t)?.idx_of(&node)?;
                let fragment = project.step_fragment_id(t, n)?;
                let f = project.attack_tree_idx(&fragment)?;
                let target = project.attack_tree(f)?;
                Some(format!(
                    "{} {}: {}",
                    catalog.tr("fragment"),
                    selection_visual_id(project, Selection::Tree(fragment)),
                    target.get(target.root())?.label(),
                ))
            })(),
        },
        _ => None,
    };
    let relations = RELATIONS
        .iter()
        .filter(|rel| Some(rel.from) == elem_kind)
        .map(|rel| InspectorRelationRow {
            key: rel.key,
            label: catalog.tr(rel.label),
            editable: rel.link.is_some(),
            chips: to_chips(project, crate::relations::related(project, rel.key, sel)),
        })
        .collect();

    let mut reverse_relations = Vec::new();
    for rel in RELATIONS {
        if !elem_kind.is_some_and(|k| rel.to.contains(&k)) {
            continue;
        }
        let sources = match (sel, rel.key) {
            (Selection::Component(cid), RelationKey::STEP_VIA) => {
                steps_through(project, ItemRef::Component(cid))
                    .into_iter()
                    .filter_map(|s| {
                        let t = project.attack_tree_idx(&s.tree)?;
                        let n = project.attack_tree(t)?.idx_of(&s.node)?;
                        project
                            .step_shared(t, n)
                            .is_none()
                            .then_some(Selection::TreeNode(s.tree, s.node))
                    })
                    .collect()
            }
            (Selection::Control(cid), RelationKey::STEP_MITIGATIONS) => {
                let mut out = Vec::new();
                if let Some(c_idx) = project.security_control_idx(&cid) {
                    for (t_idx, tree_id, tree) in project.attack_trees() {
                        for (n_idx, node_id, _) in tree.iter() {
                            if project
                                .step_ceilings(t_idx, n_idx)
                                .iter()
                                .any(|s| s.control() == c_idx)
                            {
                                out.push(Selection::TreeNode(tree_id, node_id));
                            }
                        }
                    }
                }
                out
            }
            _ => crate::relations::reverse_related(project, rel.key, sel),
        };
        if !sources.is_empty() {
            reverse_relations.push(InspectorRelationRow {
                key: rel.key,
                label: reverse_relation_label(rel.key, catalog),
                editable: false,
                chips: to_chips(project, sources),
            });
        }
    }

    Some(InspectorView {
        selection: sel,
        visual_id: selection_visual_id(project, sel),
        name: selection_label(project, sel),
        kind,
        systems,
        excluded_reason: selection_exclusion(project, sel).1,
        exclusion_causes: exclusion_causes(project, sel, catalog),
        changed,
        description,
        shared_note,
        fields,
        explain: explain_selection(ws, sel, catalog),
        primary_cause: primary_cause(ws, sel),
        relations,
        reverse_relations,
    })
}
