//! Human-readable applicability exclusion reasons and condition-switch toasts.

use std::collections::BTreeSet;

use aegis_model::{
    AssumptionId, ConditionId, Methodology, ModelError, NodeId, Op, Project, Reason, RiskLevel,
    ThreatId, TreeId,
};

use crate::inspector::reason_target;
use crate::pages;
use crate::table;
use crate::view::Selection;
use crate::workspace::Workspace;

/// Risks named one by one in a condition-switch toast; the rest are counted.
const MAX_TOAST_RISKS: usize = 3;

fn named_target(project: &Project, sel: Selection) -> String {
    let vis = pages::selection_visual_id(project, sel);
    let label = table::selection_label(project, sel);
    let vis = vis.trim();
    let label = label.trim();
    match (vis.is_empty(), label.is_empty()) {
        (true, true) => String::new(),
        (false, true) => vis.to_owned(),
        (true, false) => label.to_owned(),
        (false, false) => format!("{vis} {label}"),
    }
}

fn dotted_item(vis: &str, label: &str) -> String {
    let vis = vis.trim();
    let label = label.trim();
    match (vis.is_empty(), label.is_empty()) {
        (true, true) => String::new(),
        (false, true) => vis.to_owned(),
        (true, false) => label.to_owned(),
        (false, false) => format!("{vis} · {label}"),
    }
}

/// Returns `true` when `threat`'s inherent (or `residual`) risk is not
/// applicable because the threat or its attack tree is excluded.
pub(crate) fn threat_risk_not_applicable(ws: &Workspace, threat: ThreatId, residual: bool) -> bool {
    let res = if residual {
        ws.cache().evaluate_residual_risk(ws.project(), threat)
    } else {
        ws.cache().evaluate_risk(ws.project(), threat)
    };
    matches!(
        res,
        Err(ModelError::ThreatNotApplicable(_) | ModelError::TreeNotApplicable(_))
    )
}

/// Pairs `(item, reasons_text)` for every excluded element and every excluded
/// attack-tree leaf step in `project`.
#[must_use]
pub fn excluded_entries(project: &Project) -> Vec<(String, String)> {
    let app = project.applicability();
    let mut out = Vec::new();
    for (eref, reasons) in app.excluded_elements() {
        if let Some(sel) = reason_target(Reason::OwnerExcluded(eref)) {
            let vis = pages::selection_visual_id(project, sel);
            let label = table::selection_label(project, sel);
            out.push((dotted_item(&vis, &label), reasons_text(project, reasons)));
        }
    }
    for (tree_id, node_id, reasons) in app.excluded_nodes() {
        if let Some(tidx) = project.attack_tree_idx(&tree_id)
            && let Some(tree) = project.attack_tree(tidx)
            && let Some(nidx) = tree.idx_of(&node_id)
            && let Some(node) = tree.get(nidx)
            && node.gate_kind().is_none()
        {
            let tree_vis = project.attack_tree_visual_id(tidx).unwrap_or_default();
            out.push((
                dotted_item(&tree_vis, node.label()),
                reasons_text(project, reasons),
            ));
        }
    }
    out
}

/// Human-readable exclusion reasons for an element or tree node, joined by `"; "`.
#[must_use]
pub fn reasons_text(project: &Project, reasons: &[Reason]) -> String {
    reasons
        .iter()
        .map(|&reason| match reason {
            Reason::ConditionOff(cid) => {
                let who = named_target(project, Selection::Condition(cid));
                if who.is_empty() {
                    "Needs a condition (off)".to_owned()
                } else {
                    format!("Needs {who} (off)")
                }
            }
            Reason::AssumptionHolds(aid) => match named_target(project, Selection::Assumption(aid))
            {
                who if who.is_empty() => "An assumption holds".to_owned(),
                who => format!("Assumption {who} holds"),
            },
            Reason::OwnerExcluded(_) => {
                let who = reason_target(reason)
                    .map(|sel| named_target(project, sel))
                    .unwrap_or_default();
                if who.is_empty() {
                    "Owner is excluded".to_owned()
                } else {
                    format!("Owner {who} is excluded")
                }
            }
            Reason::ViaExcluded(_) => {
                let who = reason_target(reason)
                    .map(|sel| named_target(project, sel))
                    .unwrap_or_default();
                if who.is_empty() {
                    "Via is excluded".to_owned()
                } else {
                    format!("Via {who} is excluded")
                }
            }
            Reason::AllPathsExcluded => "Every attack path is excluded".to_owned(),
            _ => "Excluded".to_owned(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Residual risk state of a threat for before/after comparison on a condition switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskState {
    /// No residual risk level could be computed.
    Unrated,
    /// A rated residual risk level.
    Level(RiskLevel),
    /// The threat or its attack tree is excluded by applicability.
    NotApplicable,
}

#[derive(Clone, Copy, Debug)]
enum SwitchTarget {
    Condition(ConditionId),
    Assumption(AssumptionId),
}

/// Pre-edit snapshot for a condition or assumption switch, used to build the post-edit undo toast.
#[derive(Clone, Debug)]
pub struct ConditionSnapshot {
    target: SwitchTarget,
    excluded_steps: BTreeSet<(TreeId, NodeId)>,
    risks: Vec<(ThreatId, RiskState)>,
}

fn excluded_leaf_steps(project: &Project) -> BTreeSet<(TreeId, NodeId)> {
    project
        .applicability()
        .excluded_nodes()
        .filter(|&(tree_id, node_id, _)| {
            project
                .attack_tree_idx(&tree_id)
                .and_then(|ti| project.attack_tree(ti))
                .and_then(|t| t.idx_of(&node_id).and_then(|ni| t.get(ni)))
                .is_some_and(|n| n.gate_kind().is_none())
        })
        .map(|(tree_id, node_id, _)| (tree_id, node_id))
        .collect()
}

fn risk_state(ws: &Workspace, tid: ThreatId) -> RiskState {
    match ws.cache().evaluate_residual_risk(ws.project(), tid) {
        Ok(r) => r.max_risk().map_or(RiskState::Unrated, RiskState::Level),
        Err(ModelError::ThreatNotApplicable(_) | ModelError::TreeNotApplicable(_)) => {
            RiskState::NotApplicable
        }
        Err(_) => RiskState::Unrated,
    }
}

fn risk_label(m: &Methodology, state: RiskState) -> &str {
    match state {
        RiskState::Level(level) => m.risk_level_label(level).unwrap_or("unrated"),
        RiskState::Unrated => "unrated",
        RiskState::NotApplicable => "not applicable",
    }
}

fn switch_lead(p: &Project, target: SwitchTarget) -> String {
    match target {
        SwitchTarget::Condition(id) => match p.condition_idx(&id).and_then(|i| p.condition(i)) {
            Some(c) => {
                let s = if c.present() { "on" } else { "off" };
                let who = named_target(p, Selection::Condition(id));
                format!("Condition '{who}' switched {s}")
            }
            None => "Condition switched".to_owned(),
        },
        SwitchTarget::Assumption(id) => match p.assumption_idx(&id).and_then(|i| p.assumption(i)) {
            Some(a) => {
                let who = named_target(p, Selection::Assumption(id));
                if a.holds() {
                    format!("Assumption '{who}' now holds")
                } else {
                    format!("Assumption '{who}' no longer holds")
                }
            }
            None => "Assumption switched".to_owned(),
        },
    }
}

impl ConditionSnapshot {
    /// Captures excluded steps and threat risks before a condition or assumption switch.
    #[must_use]
    pub fn capture(ws: &Workspace, ops: &[Op]) -> Option<Self> {
        let target = ops.iter().find_map(|op| match op {
            Op::SetConditionPresent { id, .. } => Some(SwitchTarget::Condition(*id)),
            Op::SetAssumptionHolds { id, .. } => Some(SwitchTarget::Assumption(*id)),
            _ => None,
        })?;
        let project = ws.project();
        let excluded_steps = excluded_leaf_steps(project);
        let risks = project
            .threat_scenarios()
            .map(|(_, tid, _)| (tid, risk_state(ws, tid)))
            .collect();
        Some(Self {
            target,
            excluded_steps,
            risks,
        })
    }

    /// Formats the post-apply toast describing how many steps were excluded or
    /// included and which threat risks moved.
    #[must_use]
    pub fn toast(&self, ws: &Workspace) -> String {
        let project = ws.project();
        let m = project.methodology();
        let lead = switch_lead(project, self.target);

        let after_steps = excluded_leaf_steps(project);
        let excluded = after_steps.difference(&self.excluded_steps).count();
        let included = self.excluded_steps.difference(&after_steps).count();
        let step_word = if excluded == 1 { "step" } else { "steps" };

        let mut moved: Vec<(String, String, RiskState, RiskState)> = Vec::new();
        for &(tid, before) in &self.risks {
            let after = risk_state(ws, tid);
            if before != after {
                let idx = project.threat_scenario_idx(&tid);
                let vis = idx
                    .and_then(|i| project.threat_scenario_visual_id(i))
                    .unwrap_or_default();
                let name = idx
                    .and_then(|i| project.threat_scenario(i))
                    .map_or("", aegis_model::ThreatScenario::name)
                    .to_owned();
                moved.push((vis, name, before, after));
            }
        }
        moved.sort_by(|a, b| a.0.cmp(&b.0));

        if moved.is_empty() {
            format!("{lead}: {excluded} {step_word} excluded, {included} included, no risk moved")
        } else {
            let n = moved.len();
            let risk_word = if n == 1 { "risk" } else { "risks" };
            let mut details = moved
                .iter()
                .take(MAX_TOAST_RISKS)
                .map(|(vis, name, before, after)| {
                    format!(
                        "{vis} '{name}' {} \u{2192} {}",
                        risk_label(m, *before),
                        risk_label(m, *after),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            if n > MAX_TOAST_RISKS {
                details.push_str(&format!(", and {} more", n - MAX_TOAST_RISKS));
            }
            format!(
                "{lead}: {excluded} {step_word} excluded, {included} included, {n} {risk_word} moved ({details})"
            )
        }
    }
}
