//! Human-readable applicability exclusion reasons and condition-switch toasts.

use std::collections::BTreeSet;

use aegis_model::{
    ConditionId, Methodology, ModelError, NodeId, Op, Project, Reason, RiskLevel, ThreatId, TreeId,
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

/// Pre-edit snapshot for a condition switch, used to build the post-edit undo toast.
#[derive(Clone, Debug)]
pub struct ConditionSnapshot {
    condition: ConditionId,
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

impl ConditionSnapshot {
    /// Captures excluded steps and threat risks before applying `ops`, if `ops`
    /// contains a condition switch (`Op::SetConditionPresent`).
    #[must_use]
    pub fn capture(ws: &Workspace, ops: &[Op]) -> Option<Self> {
        let condition = ops.iter().find_map(|op| match op {
            Op::SetConditionPresent { id, .. } => Some(*id),
            _ => None,
        })?;
        let project = ws.project();
        let excluded_steps = excluded_leaf_steps(project);
        let risks = project
            .threat_scenarios()
            .map(|(_, tid, _)| (tid, risk_state(ws, tid)))
            .collect();
        Some(Self {
            condition,
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
        let lead = match project
            .condition_idx(&self.condition)
            .and_then(|i| project.condition(i))
        {
            Some(cond) => {
                let state = if cond.present() { "on" } else { "off" };
                let who = named_target(project, Selection::Condition(self.condition));
                format!("Condition '{who}' switched {state}")
            }
            None => "Condition switched".to_owned(),
        };

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
