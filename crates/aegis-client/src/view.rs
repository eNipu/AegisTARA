//! What the user sees, as plain data.
//!
//! The view model is computed from a [`Workspace`] and the user's selection.
//! It contains no toolkit types and no model indices the UI could misuse:
//! the UI shows labels, sends back *positions in the lists it was given*
//! (an option index, a threat position), and the session maps them to typed
//! model values. Replacing the UI toolkit means rewriting only the code that
//! turns this structure into widgets.
//!
//! Everything here is derived data (D10): drop it and rebuild it from the
//! workspace at any time. Building it for a project of 2,000 threats reads the
//! incremental cache, so it costs O(threats), not a re-evaluation.

use std::sync::Arc;

use aegis_canvas::{LayoutConfig, NodeKind, NodeStyle, Scene, Tone};
use aegis_model::{
    AttackTree, FeasibilityLevel, GateKind, Methodology, NodeId, NodeIdx, ThreatId, TreeId,
};

use crate::workspace::Workspace;

/// A rating shown as text with its color.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskView {
    /// Label from the methodology, for example `Risk 4`.
    pub label: String,
    /// Color bucket.
    pub tone: Tone,
}

/// One threat scenario in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatRow {
    /// Stable ID.
    pub id: ThreatId,
    /// Name.
    pub name: String,
    /// Name of the asset it targets.
    pub asset: String,
    /// Risk before controls.
    pub inherent: Option<RiskView>,
    /// Risk after controls.
    pub residual: Option<RiskView>,
    /// Whether an attack tree is attached.
    pub has_tree: bool,
}

/// The user's selection and display choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiState {
    /// The threat whose tree is shown.
    pub selected_threat: Option<ThreatId>,
    /// The selected node of that tree.
    pub selected_node: Option<NodeId>,
    /// Show the residual evaluation instead of the inherent one.
    pub show_residual: bool,
}

/// The tree on the canvas.
pub struct TreeView {
    /// The threat it belongs to.
    pub threat: ThreatId,
    /// The tree.
    pub tree: TreeId,
    /// The laid-out scene, ready for `aegis_canvas::render`.
    pub scene: Arc<Scene>,
    /// Which node is selected, as a scene position.
    pub selected: Option<NodeIdx>,
    /// Whether this is the residual view.
    pub residual: bool,
}

/// One choice in a dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// Label from the methodology.
    pub label: String,
}

/// One attack potential factor of a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorView {
    /// Factor name.
    pub label: String,
    /// Levels, with their points in the label.
    pub options: Vec<Choice>,
    /// The chosen level's position in `options`, if rated.
    pub selected: Option<usize>,
}

/// The details panel for the selected node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inspector {
    /// Tree the node is in.
    pub tree: TreeId,
    /// The node.
    pub node: NodeId,
    /// Its label.
    pub title: String,
    /// `Step`, `AND gate`, `OR gate`, or `Sequential AND gate`.
    pub kind: String,
    /// Whether the rating can be edited (steps only).
    pub is_step: bool,
    /// Feasibility levels for the dropdown.
    pub levels: Vec<Choice>,
    /// The directly set rating's position in `levels`.
    pub rating: Option<usize>,
    /// The rating the engine uses (from attack potential if complete).
    pub effective: Option<String>,
    /// Attack potential, `13 (High)`, if every factor is rated.
    pub potential: Option<String>,
    /// Factor dropdowns (steps only).
    pub factors: Vec<FactorView>,
    /// The evaluated value at this node, for gates.
    pub evaluated: Option<String>,
    /// Explain-why lines for gates: the rule and the deciding child.
    pub why: Vec<String>,
}

fn risk_view(m: &Methodology, level: aegis_model::RiskLevel) -> Option<RiskView> {
    let label = m.risk_level_label(level)?.to_owned();
    let index = m.risk_levels().position(|(l, _, _)| l == level)?;
    Some(RiskView {
        label,
        tone: Tone::from_level(index, m.risk_level_count()),
    })
}

/// The threat list with inherent and residual risk. Time O(threats), reading
/// the cache.
#[must_use]
pub fn threat_rows(ws: &Workspace) -> Vec<ThreatRow> {
    let project = ws.project();
    let m = project.methodology();
    project
        .threat_scenarios()
        .map(|(_, id, threat)| {
            let risk = |r: Result<&aegis_model::ThreatRisk, _>| {
                r.ok()
                    .and_then(aegis_model::ThreatRisk::max_risk)
                    .and_then(|l| risk_view(m, l))
            };
            ThreatRow {
                id,
                name: threat.name().to_owned(),
                asset: project
                    .asset(threat.asset())
                    .map_or_else(String::new, |a| a.name().to_owned()),
                inherent: risk(ws.cache().evaluate_risk(project, id)),
                residual: risk(ws.cache().evaluate_residual_risk(project, id)),
                has_tree: threat.attack_tree().is_some(),
            }
        })
        .collect()
}

fn level_tone(m: &Methodology, level: FeasibilityLevel) -> Tone {
    let index = m
        .feasibility_levels()
        .position(|(l, _, _)| l == level)
        .unwrap_or(0);
    Tone::from_level(index, m.feasibility_levels().len())
}

fn level_label(m: &Methodology, level: FeasibilityLevel) -> String {
    m.feasibility_level_label(level).unwrap_or("?").to_owned()
}

/// The tree of the selected threat, laid out and annotated with the
/// evaluation (inherent or residual).
///
/// Time O(n log n) in the tree size (at most 32 nodes in a project).
#[must_use]
pub fn tree_view(ws: &Workspace, ui: &UiState) -> Option<TreeView> {
    let project = ws.project();
    let m = project.methodology();
    let threat_id = ui.selected_threat?;
    let threat = project.threat_scenario(project.threat_scenario_idx(&threat_id)?)?;
    let tree_idx = threat.attack_tree()?;
    let tree_id = project.attack_tree_id(tree_idx)?;
    let tree = project.attack_tree(tree_idx)?;
    let evaluation = if ui.show_residual {
        ws.cache().evaluate_residual_tree(project, tree_id)
    } else {
        ws.cache().evaluate_tree(project, tree_id)
    }
    .ok()?;

    let style = |idx: NodeIdx| -> NodeStyle {
        let value = evaluation.get(idx).copied();
        let is_step = tree.get(idx).is_some_and(|n| n.gate_kind().is_none());
        let score = if is_step {
            let potential = project.step_attack_potential(tree_idx, idx);
            match (potential, value) {
                (Some(p), Some(v)) => Some(format!("{} · {}", p.score(), level_label(m, v))),
                (None, Some(v)) => Some(level_label(m, v)),
                _ => Some("unrated".to_owned()),
            }
        } else {
            value.map(|v| level_label(m, v))
        };
        NodeStyle {
            score,
            tone: value.map_or(Tone::Unrated, |v| level_tone(m, v)),
            critical: false,
        }
    };
    let mut scene = Scene::from_tree(tree, LayoutConfig::default(), style);
    let path: Vec<NodeIdx> = evaluation.why_path(tree.root()).collect();
    scene.set_critical_path(&path);

    let selected = ui.selected_node.and_then(|n| tree.idx_of(&n));
    Some(TreeView {
        threat: threat_id,
        tree: tree_id,
        scene: Arc::new(scene),
        selected,
        residual: ui.show_residual,
    })
}

fn kind_text(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Step => "Step",
        NodeKind::Gate(GateKind::And) => "AND gate",
        NodeKind::Gate(GateKind::Or) => "OR gate",
        NodeKind::Gate(GateKind::SeqAnd) => "Sequential AND gate",
    }
}

/// The details of the selected node.
///
/// Time O(node degree + factors).
#[must_use]
pub fn inspector(ws: &Workspace, ui: &UiState) -> Option<Inspector> {
    let project = ws.project();
    let m = project.methodology();
    let threat_id = ui.selected_threat?;
    let threat = project.threat_scenario(project.threat_scenario_idx(&threat_id)?)?;
    let tree_idx = threat.attack_tree()?;
    let tree_id = project.attack_tree_id(tree_idx)?;
    let tree: &AttackTree = project.attack_tree(tree_idx)?;
    let node_id = ui.selected_node?;
    let idx = tree.idx_of(&node_id)?;
    let node = tree.get(idx)?;
    let kind = node.gate_kind().map_or(NodeKind::Step, NodeKind::Gate);
    let is_step = kind == NodeKind::Step;
    let evaluation = if ui.show_residual {
        ws.cache().evaluate_residual_tree(project, tree_id)
    } else {
        ws.cache().evaluate_tree(project, tree_id)
    }
    .ok();
    let value = evaluation.and_then(|e| e.get(idx).copied());

    let levels: Vec<Choice> = m
        .feasibility_levels()
        .map(|(_, _, label)| Choice {
            label: label.to_owned(),
        })
        .collect();
    let direct = project.step_feasibility(tree_idx, idx);
    let rating = direct.and_then(|d| m.feasibility_levels().position(|(l, _, _)| l == d));
    let potential = project.step_attack_potential(tree_idx, idx);

    let factors = if is_step {
        m.feasibility_factors()
            .map(|f| {
                let current = project.step_factor(tree_idx, idx, f);
                FactorView {
                    label: m.feasibility_factor_label(f).unwrap_or("?").to_owned(),
                    options: m
                        .factor_levels(f)
                        .map(|l| Choice {
                            label: format!(
                                "{} ({})",
                                m.factor_level_label(f, l).unwrap_or("?"),
                                m.factor_level_points(f, l).unwrap_or(0)
                            ),
                        })
                        .collect(),
                    selected: current.and_then(|c| m.factor_levels(f).position(|l| l == c)),
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut why = Vec::new();
    if let Some(e) = evaluation
        && let Some(explanation) = e.explain(tree, idx)
        && !is_step
    {
        let rule = match explanation.rule() {
            Some(GateKind::And) => "AND: the hardest child decides",
            Some(GateKind::Or) => "OR: the easiest child decides",
            Some(GateKind::SeqAnd) => "Sequential AND: the hardest step decides",
            None => "",
        };
        if !rule.is_empty() {
            why.push(rule.to_owned());
        }
        if let Some(decisive) = explanation.decisive()
            && let Some(child) = tree.get(decisive)
        {
            why.push(format!(
                "Decided by \"{}\" ({})",
                child.label(),
                e.get(decisive)
                    .map_or_else(|| "unrated".to_owned(), |v| level_label(m, *v))
            ));
        }
    }

    Some(Inspector {
        tree: tree_id,
        node: node_id,
        title: node.label().to_owned(),
        kind: kind_text(kind).to_owned(),
        is_step,
        levels,
        rating,
        effective: project
            .effective_step_feasibility(tree_idx, idx)
            .filter(|_| is_step)
            .map(|l| level_label(m, l)),
        potential: potential
            .map(|p| format!("{} ({})", p.score(), level_label(m, p.feasibility()))),
        factors,
        evaluated: value.map(|v| level_label(m, v)),
        why,
    })
}
