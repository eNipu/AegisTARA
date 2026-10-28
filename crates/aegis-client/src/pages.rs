//! The pages of an open project, as plain data (job 040,
//! docs/adr/0038-tara-pages.md).
//!
//! Every page of the TARA flow (direction section 11) has a builder here:
//! a pure function from the [`Workspace`] and the user's selection to rows
//! of text and the IDs behind them. The session builds only the page on
//! screen, so a publish costs O(entities of that page), never the whole
//! project (budget: interaction within 50 ms on the `large` project).
//!
//! Like the rest of the view model, nothing here is a source of truth (D10):
//! drop it and rebuild it at any time. Names appear in these rows because
//! they are for the user's own screen; the model's diagnostics never carry
//! names (R6), so [`problems`] adds them here.

use aegis_canvas::Tone;
use aegis_model::{
    AssetId, ClaimId, ControlId, DamageId, GoalId, LintCode, LintSeverity, Methodology, Project,
    Provenance, ThreatId, TreeId,
};

use crate::commands::TREATMENTS;
use crate::time::format_ms;
use crate::view::{RiskView, Selection, UiState};
use crate::workspace::Workspace;

/// The labels of the project's methodology, in the order edits refer to
/// them by position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vocabulary {
    /// Security properties of assets.
    pub properties: Vec<String>,
    /// Impact categories of damage scenarios, with their levels.
    pub impact_categories: Vec<ImpactCategoryView>,
    /// Threat categories (STRIDE in the built-in pack).
    pub threat_categories: Vec<String>,
    /// Attack feasibility levels.
    pub feasibility: Vec<String>,
    /// Risk levels.
    pub risk_levels: Vec<String>,
    /// Catalog schemes (UN R155 Annex 5 in the built-in pack).
    pub catalog_schemes: Vec<String>,
    /// Treatment decisions, as [`TREATMENTS`] orders them.
    pub treatments: Vec<String>,
}

/// One impact category and its levels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImpactCategoryView {
    /// Label.
    pub label: String,
    /// Level labels, lowest first.
    pub levels: Vec<String>,
}

/// The vocabulary of a methodology. Time O(vocabulary size).
#[must_use]
pub fn vocabulary(m: &Methodology) -> Vocabulary {
    Vocabulary {
        properties: m
            .properties()
            .map(|p| m.property_label(p).unwrap_or("?").to_owned())
            .collect(),
        impact_categories: m
            .impact_categories()
            .map(|c| ImpactCategoryView {
                label: m.impact_category_label(c).unwrap_or("?").to_owned(),
                levels: m
                    .levels(c)
                    .map(|l| m.level_label(c, l).unwrap_or("?").to_owned())
                    .collect(),
            })
            .collect(),
        threat_categories: m
            .threat_categories()
            .map(|t| m.threat_category_label(t).unwrap_or("?").to_owned())
            .collect(),
        feasibility: m
            .feasibility_levels()
            .map(|(_, _, label)| label.to_owned())
            .collect(),
        risk_levels: m
            .risk_levels()
            .map(|(_, _, label)| label.to_owned())
            .collect(),
        catalog_schemes: m
            .catalog_schemes()
            .map(|s| m.catalog_scheme_label(s).unwrap_or("?").to_owned())
            .collect(),
        treatments: TREATMENTS
            .iter()
            .map(|t| treatment_label(*t).to_owned())
            .collect(),
    }
}

/// The word for a treatment decision.
#[must_use]
pub fn treatment_label(t: aegis_model::RiskTreatment) -> &'static str {
    match t {
        aegis_model::RiskTreatment::Avoid => "Avoid",
        aegis_model::RiskTreatment::Reduce => "Reduce",
        aegis_model::RiskTreatment::Share => "Share",
        aegis_model::RiskTreatment::Retain => "Retain",
    }
}

/// "Changed by alice on 25 Oct 2026 19:30 UTC", or empty for no provenance.
#[must_use]
pub fn changed_text(p: &Provenance) -> String {
    if p.actor().is_empty() {
        return String::new();
    }
    let source = match p.source() {
        aegis_model::ProvenanceSource::Human => "",
        aegis_model::ProvenanceSource::Import => " (imported)",
        aegis_model::ProvenanceSource::Catalog => " (from a catalog)",
        aegis_model::ProvenanceSource::Ai => " (AI suggestion)",
    };
    format!(
        "Last changed by {} on {}{source}",
        p.actor(),
        format_ms(p.timestamp_ms())
    )
}

/// A row that names a related entity and leads to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRow {
    /// Main text.
    pub label: String,
    /// Second text (a rating, a kind).
    pub detail: String,
    /// Where a click goes.
    pub target: Selection,
}

fn risk_view(m: &Methodology, level: aegis_model::RiskLevel) -> Option<RiskView> {
    let label = m.risk_level_label(level)?.to_owned();
    let index = m.risk_levels().position(|(l, _, _)| l == level)?;
    Some(RiskView {
        label,
        tone: Tone::from_level(index, m.risk_level_count()),
    })
}

fn feasibility_position(m: &Methodology, level: aegis_model::FeasibilityLevel) -> Option<usize> {
    m.feasibility_levels().position(|(l, _, _)| l == level)
}

fn feasibility_label(m: &Methodology, level: aegis_model::FeasibilityLevel) -> String {
    m.feasibility_level_label(level).unwrap_or("?").to_owned()
}

/// Worst inherent and residual risk of a threat, from the cache.
fn threat_risks(ws: &Workspace, id: ThreatId) -> (Option<RiskView>, Option<RiskView>) {
    let project = ws.project();
    let m = project.methodology();
    let view = |r: Result<&aegis_model::ThreatRisk, aegis_model::ModelError>| {
        r.ok()
            .and_then(aegis_model::ThreatRisk::max_risk)
            .and_then(|l| risk_view(m, l))
    };
    (
        view(ws.cache().evaluate_risk(project, id)),
        view(ws.cache().evaluate_residual_risk(project, id)),
    )
}

fn worse(a: Option<RiskView>, b: Option<RiskView>) -> Option<RiskView> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b.tone > a.tone { b } else { a }),
        (a, b) => a.or(b),
    }
}

// ---------------------------------------------------------------------------
// Assets.
// ---------------------------------------------------------------------------

/// One asset in the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRow {
    /// ID.
    pub id: AssetId,
    /// Name.
    pub name: String,
    /// One flag per methodology property.
    pub properties: Vec<bool>,
    /// Threats that target it.
    pub threats: usize,
    /// The worst inherent risk among them.
    pub worst: Option<RiskView>,
}

/// The selected asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetDetail {
    /// ID.
    pub id: AssetId,
    /// Name.
    pub name: String,
    /// One flag per methodology property.
    pub properties: Vec<bool>,
    /// The threats that target it.
    pub threats: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
}

/// Every asset in arena order. Time O(assets x properties + threats).
#[must_use]
pub fn asset_rows(ws: &Workspace) -> Vec<AssetRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut counts = vec![(0usize, None::<RiskView>); project.asset_count().max(1)];
    let mut positions = std::collections::HashMap::new();
    for (pos, (idx, _, _)) in project.assets().enumerate() {
        positions.insert(idx, pos);
    }
    for (_, id, t) in project.threat_scenarios() {
        if let Some(&pos) = positions.get(&t.asset())
            && let Some(slot) = counts.get_mut(pos)
        {
            slot.0 += 1;
            slot.1 = worse(slot.1.take(), threat_risks(ws, id).0);
        }
    }
    project
        .assets()
        .enumerate()
        .map(|(pos, (_, id, a))| {
            let (threats, worst) = counts.get(pos).cloned().unwrap_or_default();
            AssetRow {
                id,
                name: a.name().to_owned(),
                properties: m.properties().map(|p| a.properties().contains(p)).collect(),
                threats,
                worst,
            }
        })
        .collect()
}

/// The selected asset's details. Time O(threats).
#[must_use]
pub fn asset_detail(ws: &Workspace, id: AssetId) -> Option<AssetDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.asset_idx(&id)?;
    let a = project.asset(idx)?;
    Some(AssetDetail {
        id,
        name: a.name().to_owned(),
        properties: m.properties().map(|p| a.properties().contains(p)).collect(),
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.asset() == idx)
            .map(|(_, tid, t)| LinkRow {
                label: t.name().to_owned(),
                detail: threat_risks(ws, tid)
                    .0
                    .map_or_else(|| "not rated".to_owned(), |r| r.label),
                target: Selection::Threat(tid),
            })
            .collect(),
        changed: changed_text(a.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Damage scenarios.
// ---------------------------------------------------------------------------

/// One damage scenario in the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageRow {
    /// ID.
    pub id: DamageId,
    /// Name.
    pub name: String,
    /// Per impact category: the level's position, if rated.
    pub impacts: Vec<Option<usize>>,
    /// The ratings as text, for example `Safety: Severe`.
    pub impacts_text: String,
    /// Threats that link it.
    pub threats: usize,
}

/// The selected damage scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageDetail {
    /// ID.
    pub id: DamageId,
    /// Name.
    pub name: String,
    /// Per impact category: the level's position, if rated.
    pub impacts: Vec<Option<usize>>,
    /// The threats that link it.
    pub threats: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
}

fn impacts(m: &Methodology, d: &aegis_model::DamageScenario) -> (Vec<Option<usize>>, String) {
    let mut positions = Vec::with_capacity(m.impact_category_count());
    let mut text = Vec::new();
    for cat in m.impact_categories() {
        let level = d.impact(cat);
        positions.push(level.and_then(|l| m.levels(cat).position(|x| x == l)));
        if let Some(l) = level {
            text.push(format!(
                "{}: {}",
                m.impact_category_label(cat).unwrap_or("?"),
                m.level_label(cat, l).unwrap_or("?")
            ));
        }
    }
    (positions, text.join(", "))
}

/// Every damage scenario. Time O(damage x categories + threats x links).
#[must_use]
pub fn damage_rows(ws: &Workspace) -> Vec<DamageRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut links = std::collections::HashMap::new();
    for (_, _, t) in project.threat_scenarios() {
        for d in t.damages() {
            *links.entry(*d).or_insert(0usize) += 1;
        }
    }
    project
        .damage_scenarios()
        .map(|(idx, id, d)| {
            let (impacts, impacts_text) = impacts(m, d);
            DamageRow {
                id,
                name: d.name().to_owned(),
                impacts,
                impacts_text,
                threats: links.get(&idx).copied().unwrap_or(0),
            }
        })
        .collect()
}

/// The selected damage scenario's details.
#[must_use]
pub fn damage_detail(ws: &Workspace, id: DamageId) -> Option<DamageDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.damage_scenario_idx(&id)?;
    let d = project.damage_scenario(idx)?;
    Some(DamageDetail {
        id,
        name: d.name().to_owned(),
        impacts: impacts(m, d).0,
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.damages().contains(&idx))
            .map(|(_, tid, t)| LinkRow {
                label: t.name().to_owned(),
                detail: String::new(),
                target: Selection::Threat(tid),
            })
            .collect(),
        changed: changed_text(d.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Threat scenarios and the risk register.
// ---------------------------------------------------------------------------

/// A damage scenario as seen from a threat: linked or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageLink {
    /// The damage scenario.
    pub id: DamageId,
    /// Its name.
    pub name: String,
    /// Whether the threat links it.
    pub linked: bool,
}

/// A goal or claim offered in a dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice<Id> {
    /// The entity.
    pub id: Id,
    /// Its name.
    pub name: String,
}

/// The selected threat scenario, with everything that decides its risk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatDetail {
    /// ID.
    pub id: ThreatId,
    /// Name.
    pub name: String,
    /// The targeted asset.
    pub asset: AssetId,
    /// Its name.
    pub asset_name: String,
    /// Position in the threat categories.
    pub category: usize,
    /// Catalog references as text.
    pub catalog_refs: Vec<String>,
    /// Every damage scenario of the project, linked or not.
    pub damages: Vec<DamageLink>,
    /// Direct feasibility rating, as a position in the levels.
    pub direct_feasibility: Option<usize>,
    /// The linked attack tree.
    pub tree: Option<TreeId>,
    /// The trees that can be linked.
    pub trees: Vec<Choice<TreeId>>,
    /// Treatment, as a position in [`TREATMENTS`].
    pub treatment: Option<usize>,
    /// The linked goal.
    pub goal: Option<GoalId>,
    /// Every goal.
    pub goals: Vec<Choice<GoalId>>,
    /// The linked claim.
    pub claim: Option<ClaimId>,
    /// Every claim.
    pub claims: Vec<Choice<ClaimId>>,
    /// The feasibility the engine uses (from the tree, else direct).
    pub feasibility: Option<String>,
    /// Residual feasibility (after controls).
    pub residual_feasibility: Option<String>,
    /// Inherent risk.
    pub inherent: Option<RiskView>,
    /// Residual risk.
    pub residual: Option<RiskView>,
    /// How the risk was found, step by step.
    pub why: Vec<String>,
    /// Provenance text.
    pub changed: String,
}

fn explain_risk(
    project: &Project,
    risk: Result<&aegis_model::ThreatRisk, aegis_model::ModelError>,
    what: &str,
) -> Vec<String> {
    let m = project.methodology();
    let Ok(risk) = risk else {
        return vec![format!(
            "{what}: cannot be computed yet. It needs a feasibility (an attack tree or a direct rating) and at least one rated damage scenario."
        )];
    };
    let Some(entry) = risk.deciding_entry() else {
        return vec![format!("{what}: no rated damage scenario is linked.")];
    };
    let damage = project
        .damage_scenario_idx(&entry.damage())
        .and_then(|i| project.damage_scenario(i))
        .map_or("?", |d| d.name());
    let cat = entry.category();
    vec![format!(
        "{what}: {} = {} impact \"{}\" ({}) x feasibility {}. The worst of {} damage rating(s) decides.",
        m.risk_level_label(entry.risk()).unwrap_or("?"),
        m.impact_category_label(cat).unwrap_or("?"),
        damage,
        m.level_label(cat, entry.impact()).unwrap_or("?"),
        feasibility_label(m, entry.feasibility()),
        risk.entries().len(),
    )]
}

/// The selected threat's details. Time O(damage + trees + goals + claims).
#[must_use]
pub fn threat_detail(ws: &Workspace, id: ThreatId) -> Option<ThreatDetail> {
    let project = ws.project();
    let m = project.methodology();
    let t = project.threat_scenario(project.threat_scenario_idx(&id)?)?;
    let asset_idx = t.asset();
    let (inherent, residual) = threat_risks(ws, id);
    let mut why = explain_risk(
        project,
        ws.cache().evaluate_risk(project, id),
        "Risk before controls",
    );
    why.extend(explain_risk(
        project,
        ws.cache().evaluate_residual_risk(project, id),
        "Risk after controls",
    ));
    if t.attack_tree().is_some() && t.direct_feasibility().is_some() {
        why.push(
            "The attack tree decides the feasibility; the direct rating is kept but not used."
                .to_owned(),
        );
    }
    let feasibility = ws
        .cache()
        .evaluate_threat(project, id)
        .ok()
        .map(|e| feasibility_label(m, *e.root_value()));
    let residual_feasibility = ws
        .cache()
        .evaluate_residual_threat(project, id)
        .ok()
        .map(|e| feasibility_label(m, *e.root_value()));
    Some(ThreatDetail {
        id,
        name: t.name().to_owned(),
        asset: project.asset_id(asset_idx)?,
        asset_name: project
            .asset(asset_idx)
            .map(|a| a.name().to_owned())
            .unwrap_or_default(),
        category: m
            .threat_categories()
            .position(|c| c == t.category())
            .unwrap_or(0),
        catalog_refs: t
            .catalog_refs()
            .iter()
            .map(|r| {
                format!(
                    "{} {}",
                    m.catalog_scheme_label(r.scheme()).unwrap_or("?"),
                    m.display_catalog_ref(r)
                )
            })
            .collect(),
        damages: project
            .damage_scenarios()
            .map(|(idx, did, d)| DamageLink {
                id: did,
                name: d.name().to_owned(),
                linked: t.damages().contains(&idx),
            })
            .collect(),
        direct_feasibility: t
            .direct_feasibility()
            .and_then(|l| feasibility_position(m, l)),
        tree: t.attack_tree().and_then(|i| project.attack_tree_id(i)),
        trees: project
            .attack_trees()
            .map(|(_, tid, tree)| Choice {
                id: tid,
                name: tree
                    .get(tree.root())
                    .map(|n| n.label().to_owned())
                    .unwrap_or_default(),
            })
            .collect(),
        treatment: t
            .treatment()
            .and_then(|x| TREATMENTS.iter().position(|y| *y == x)),
        goal: t.goal().and_then(|i| project.cybersecurity_goal_id(i)),
        goals: project
            .cybersecurity_goals()
            .map(|(_, gid, g)| Choice {
                id: gid,
                name: g.name().to_owned(),
            })
            .collect(),
        claim: t.claim().and_then(|i| project.cybersecurity_claim_id(i)),
        claims: project
            .cybersecurity_claims()
            .map(|(_, cid, c)| Choice {
                id: cid,
                name: c.name().to_owned(),
            })
            .collect(),
        feasibility,
        residual_feasibility,
        inherent,
        residual,
        why,
        changed: changed_text(t.provenance()),
    })
}

/// One line of the risk register.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskRow {
    /// The threat.
    pub id: ThreatId,
    /// Its name.
    pub threat: String,
    /// The asset's name.
    pub asset: String,
    /// The damage scenario that decides the risk.
    pub damage: String,
    /// Its impact, for example `Safety: Severe`.
    pub impact: String,
    /// Attack feasibility used.
    pub feasibility: String,
    /// Risk before controls.
    pub inherent: Option<RiskView>,
    /// Risk after controls.
    pub residual: Option<RiskView>,
    /// Treatment decision.
    pub treatment: String,
    /// The goal or claim that backs the treatment.
    pub backing: String,
}

/// The risk register: every threat, worst risk first (ties in project
/// order). Time O(threats log threats).
#[must_use]
pub fn risk_rows(ws: &Workspace) -> Vec<RiskRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut rows: Vec<(usize, RiskRow)> = project
        .threat_scenarios()
        .enumerate()
        .map(|(pos, (_, id, t))| {
            let (inherent, residual) = threat_risks(ws, id);
            let deciding = ws
                .cache()
                .evaluate_risk(project, id)
                .ok()
                .and_then(|r| r.deciding_entry().copied());
            let (damage, impact) = deciding
                .map(|e| {
                    let d = project
                        .damage_scenario_idx(&e.damage())
                        .and_then(|i| project.damage_scenario(i))
                        .map_or_else(String::new, |d| d.name().to_owned());
                    let c = e.category();
                    (
                        d,
                        format!(
                            "{}: {}",
                            m.impact_category_label(c).unwrap_or("?"),
                            m.level_label(c, e.impact()).unwrap_or("?")
                        ),
                    )
                })
                .unwrap_or_default();
            let backing = t
                .goal()
                .and_then(|g| project.cybersecurity_goal(g))
                .map(|g| format!("Goal: {}", g.name()))
                .or_else(|| {
                    t.claim()
                        .and_then(|c| project.cybersecurity_claim(c))
                        .map(|c| format!("Claim: {}", c.name()))
                })
                .unwrap_or_default();
            (
                pos,
                RiskRow {
                    id,
                    threat: t.name().to_owned(),
                    asset: project
                        .asset(t.asset())
                        .map(|a| a.name().to_owned())
                        .unwrap_or_default(),
                    damage,
                    impact,
                    feasibility: ws
                        .cache()
                        .evaluate_threat(project, id)
                        .ok()
                        .map(|e| feasibility_label(m, *e.root_value()))
                        .unwrap_or_default(),
                    inherent,
                    residual,
                    treatment: t
                        .treatment()
                        .map(treatment_label)
                        .unwrap_or_default()
                        .to_owned(),
                    backing,
                },
            )
        })
        .collect();
    let rank = |r: &RiskRow| r.inherent.as_ref().map_or(Tone::Unrated, |v| v.tone);
    rows.sort_by(|(pa, a), (pb, b)| rank(b).cmp(&rank(a)).then(pa.cmp(pb)));
    rows.into_iter().map(|(_, r)| r).collect()
}

// ---------------------------------------------------------------------------
// Attack trees.
// ---------------------------------------------------------------------------

/// One attack tree in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// ID.
    pub id: TreeId,
    /// The root's text.
    pub title: String,
    /// Number of nodes.
    pub nodes: usize,
    /// Names of the linked threats.
    pub threats: String,
    /// The evaluated feasibility at the root.
    pub feasibility: String,
}

/// Every attack tree. Time O(trees + threats).
#[must_use]
pub fn tree_rows(ws: &Workspace) -> Vec<TreeRow> {
    let project = ws.project();
    let m = project.methodology();
    project
        .attack_trees()
        .map(|(idx, id, tree)| {
            let threats: Vec<&str> = project
                .threat_scenarios()
                .filter(|(_, _, t)| t.attack_tree() == Some(idx))
                .map(|(_, _, t)| t.name())
                .collect();
            TreeRow {
                id,
                title: tree
                    .get(tree.root())
                    .map(|n| n.label().to_owned())
                    .unwrap_or_default(),
                nodes: tree.len(),
                threats: threats.join(", "),
                feasibility: ws
                    .cache()
                    .evaluate_tree(project, id)
                    .ok()
                    .and_then(|e| e.get(tree.root()).copied())
                    .map(|l| feasibility_label(m, l))
                    .unwrap_or_else(|| "not rated".to_owned()),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Controls, goals, claims.
// ---------------------------------------------------------------------------

/// One security control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRow {
    /// ID.
    pub id: ControlId,
    /// Name.
    pub name: String,
    /// Steps it is attached to.
    pub steps: usize,
    /// Goals it belongs to.
    pub goals: usize,
}

/// The selected control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlDetail {
    /// ID.
    pub id: ControlId,
    /// Name.
    pub name: String,
    /// Where it is attached: step, tree and the cap.
    pub steps: Vec<LinkRow>,
    /// Goals that list it.
    pub goals: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
}

fn control_uses(project: &Project, c_idx: aegis_model::ControlIdx) -> Vec<LinkRow> {
    let m = project.methodology();
    let mut out = Vec::new();
    for (t_idx, tree_id, tree) in project.attack_trees() {
        let title = tree.get(tree.root()).map_or("", |n| n.label());
        for (n_idx, _, node) in tree.iter() {
            for s in project.step_mitigations(t_idx, n_idx) {
                if s.control() == c_idx {
                    out.push(LinkRow {
                        label: node.label().to_owned(),
                        detail: format!(
                            "in \"{title}\", caps feasibility at {}",
                            feasibility_label(m, s.feasibility())
                        ),
                        target: Selection::Tree(tree_id),
                    });
                }
            }
        }
    }
    out
}

/// Every control. Time O(controls + tree nodes + goals).
#[must_use]
pub fn control_rows(ws: &Workspace) -> Vec<ControlRow> {
    let project = ws.project();
    let mut steps = std::collections::HashMap::new();
    for (t_idx, _, tree) in project.attack_trees() {
        for (n_idx, _, _) in tree.iter() {
            for s in project.step_mitigations(t_idx, n_idx) {
                *steps.entry(s.control()).or_insert(0usize) += 1;
            }
        }
    }
    let mut goals = std::collections::HashMap::new();
    for (_, _, g) in project.cybersecurity_goals() {
        for c in g.controls() {
            *goals.entry(*c).or_insert(0usize) += 1;
        }
    }
    project
        .security_controls()
        .map(|(idx, id, c)| ControlRow {
            id,
            name: c.name().to_owned(),
            steps: steps.get(&idx).copied().unwrap_or(0),
            goals: goals.get(&idx).copied().unwrap_or(0),
        })
        .collect()
}

/// The selected control's details.
#[must_use]
pub fn control_detail(ws: &Workspace, id: ControlId) -> Option<ControlDetail> {
    let project = ws.project();
    let idx = project.security_control_idx(&id)?;
    let c = project.security_control(idx)?;
    Some(ControlDetail {
        id,
        name: c.name().to_owned(),
        steps: control_uses(project, idx),
        goals: project
            .cybersecurity_goals()
            .filter(|(_, _, g)| g.controls().contains(&idx))
            .map(|(_, gid, g)| LinkRow {
                label: g.name().to_owned(),
                detail: String::new(),
                target: Selection::Goal(gid),
            })
            .collect(),
        changed: changed_text(c.provenance()),
    })
}

/// A goal row.
pub type GoalRow = GoalRowOf<GoalId>;
/// A claim row.
pub type ClaimRow = GoalRowOf<ClaimId>;

/// One goal or claim in a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalRowOf<Id> {
    /// ID.
    pub id: Id,
    /// Name or statement.
    pub name: String,
    /// Linked controls (goals) or 0 (claims).
    pub controls: usize,
    /// Threats that reference it.
    pub threats: usize,
}

/// A control as seen from a goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlLink {
    /// The control.
    pub id: ControlId,
    /// Its name.
    pub name: String,
    /// Whether the goal lists it.
    pub linked: bool,
}

/// The selected goal or claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalDetail<Id> {
    /// ID.
    pub id: Id,
    /// Name or statement.
    pub name: String,
    /// Every control and whether it is linked (goals only).
    pub controls: Vec<ControlLink>,
    /// Threats that reference it.
    pub threats: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
}

/// Every goal. Time O(goals + threats).
#[must_use]
pub fn goal_rows(ws: &Workspace) -> Vec<GoalRow> {
    let project = ws.project();
    project
        .cybersecurity_goals()
        .map(|(idx, id, g)| GoalRowOf {
            id,
            name: g.name().to_owned(),
            controls: g.controls().len(),
            threats: project
                .threat_scenarios()
                .filter(|(_, _, t)| t.goal() == Some(idx))
                .count(),
        })
        .collect()
}

/// Every claim. Time O(claims + threats).
#[must_use]
pub fn claim_rows(ws: &Workspace) -> Vec<ClaimRow> {
    let project = ws.project();
    project
        .cybersecurity_claims()
        .map(|(idx, id, c)| GoalRowOf {
            id,
            name: c.name().to_owned(),
            controls: 0,
            threats: project
                .threat_scenarios()
                .filter(|(_, _, t)| t.claim() == Some(idx))
                .count(),
        })
        .collect()
}

/// The selected goal's details.
#[must_use]
pub fn goal_detail(ws: &Workspace, id: GoalId) -> Option<GoalDetail<GoalId>> {
    let project = ws.project();
    let idx = project.cybersecurity_goal_idx(&id)?;
    let g = project.cybersecurity_goal(idx)?;
    Some(GoalDetail {
        id,
        name: g.name().to_owned(),
        controls: project
            .security_controls()
            .map(|(c_idx, cid, c)| ControlLink {
                id: cid,
                name: c.name().to_owned(),
                linked: g.controls().contains(&c_idx),
            })
            .collect(),
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.goal() == Some(idx))
            .map(|(_, tid, t)| LinkRow {
                label: t.name().to_owned(),
                detail: t
                    .treatment()
                    .map(treatment_label)
                    .unwrap_or_default()
                    .to_owned(),
                target: Selection::Threat(tid),
            })
            .collect(),
        changed: changed_text(g.provenance()),
    })
}

/// The selected claim's details.
#[must_use]
pub fn claim_detail(ws: &Workspace, id: ClaimId) -> Option<GoalDetail<ClaimId>> {
    let project = ws.project();
    let idx = project.cybersecurity_claim_idx(&id)?;
    let c = project.cybersecurity_claim(idx)?;
    Some(GoalDetail {
        id,
        name: c.name().to_owned(),
        controls: Vec::new(),
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.claim() == Some(idx))
            .map(|(_, tid, t)| LinkRow {
                label: t.name().to_owned(),
                detail: t
                    .treatment()
                    .map(treatment_label)
                    .unwrap_or_default()
                    .to_owned(),
                target: Selection::Threat(tid),
            })
            .collect(),
        changed: changed_text(c.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Problems.
// ---------------------------------------------------------------------------

/// One lint finding, in words, with a way to get to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProblemRow {
    /// Blocks evaluation or treatment (error) or not (warning).
    pub error: bool,
    /// What is wrong, naming the entity.
    pub text: String,
    /// The entity to open.
    pub target: Option<Selection>,
}

fn named(project: &Project, s: Selection) -> &str {
    match s {
        Selection::Asset(id) => project
            .asset_idx(&id)
            .and_then(|i| project.asset(i))
            .map(aegis_model::Asset::name),
        Selection::Damage(id) => project
            .damage_scenario_idx(&id)
            .and_then(|i| project.damage_scenario(i))
            .map(aegis_model::DamageScenario::name),
        Selection::Threat(id) => project
            .threat_scenario_idx(&id)
            .and_then(|i| project.threat_scenario(i))
            .map(aegis_model::ThreatScenario::name),
        Selection::Tree(id) => project
            .attack_tree_idx(&id)
            .and_then(|i| project.attack_tree(i))
            .and_then(|t| t.get(t.root()))
            .map(aegis_model::Node::label),
        Selection::TreeNode(tree, node) => project
            .attack_tree_idx(&tree)
            .and_then(|i| project.attack_tree(i))
            .and_then(|t| t.idx_of(&node).and_then(|n| t.get(n)))
            .map(aegis_model::Node::label),
        Selection::Control(id) => project
            .security_control_idx(&id)
            .and_then(|i| project.security_control(i))
            .map(aegis_model::SecurityControl::name),
        Selection::Goal(id) => project
            .cybersecurity_goal_idx(&id)
            .and_then(|i| project.cybersecurity_goal(i))
            .map(aegis_model::CybersecurityGoal::name),
        Selection::Claim(id) => project
            .cybersecurity_claim_idx(&id)
            .and_then(|i| project.cybersecurity_claim(i))
            .map(aegis_model::CybersecurityClaim::name),
    }
    .unwrap_or("?")
}

fn node_label(project: &Project, tree: TreeId, node: aegis_model::NodeId) -> String {
    project
        .attack_tree_idx(&tree)
        .and_then(|i| project.attack_tree(i))
        .and_then(|t| t.idx_of(&node).and_then(|n| t.get(n)))
        .map_or_else(|| "?".to_owned(), |n| n.label().to_owned())
}

/// Turns one lint code into a sentence and a target.
#[must_use]
pub fn problem(project: &Project, code: LintCode) -> ProblemRow {
    use LintCode as L;
    let m = project.methodology();
    let q = |s: Selection| format!("\"{}\"", named(project, s));
    let (text, target) = match code {
        L::AssetWithoutThreats { asset } => {
            let s = Selection::Asset(asset);
            (format!("Asset {} has no threat scenarios.", q(s)), s)
        }
        L::AssetPropertyUntested { asset, property } => {
            let s = Selection::Asset(asset);
            (
                format!(
                    "No threat scenario on asset {} violates its property \"{}\".",
                    q(s),
                    m.property_label(property).unwrap_or("?")
                ),
                s,
            )
        }
        L::DamageScenarioWithoutImpacts { damage } => {
            let s = Selection::Damage(damage);
            (format!("Damage scenario {} has no impact rating.", q(s)), s)
        }
        L::DamageScenarioUnlinked { damage } => {
            let s = Selection::Damage(damage);
            (
                format!("No threat scenario leads to damage scenario {}.", q(s)),
                s,
            )
        }
        L::ThreatWithoutDamages { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} links no damage scenario, so it has no risk.",
                    q(s)
                ),
                s,
            )
        }
        L::ThreatWithoutFeasibility { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} has no attack tree and no direct feasibility rating.",
                    q(s)
                ),
                s,
            )
        }
        L::RedundantDirectFeasibility { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} has a direct rating that its attack tree overrides.",
                    q(s)
                ),
                s,
            )
        }
        L::ThreatWithoutTreatment { threat } => {
            let s = Selection::Threat(threat);
            (
                format!("Threat scenario {} has no treatment decision.", q(s)),
                s,
            )
        }
        L::ReduceTreatmentWithoutControls { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} is reduced, but no control mitigates any of its attack steps.",
                    q(s)
                ),
                s,
            )
        }
        L::ReduceTreatmentWithoutGoal { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} is reduced but has no cybersecurity goal.",
                    q(s)
                ),
                s,
            )
        }
        L::RetainOrShareTreatmentWithoutClaim { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} is retained or shared but has no cybersecurity claim.",
                    q(s)
                ),
                s,
            )
        }
        L::GoalOnNonReduceThreat { threat, goal } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} has goal {} but its treatment is not Reduce.",
                    q(s),
                    q(Selection::Goal(goal))
                ),
                s,
            )
        }
        L::ClaimOnNonRetainOrShareThreat { threat, claim } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "Threat scenario {} has claim {} but its treatment is not Retain or Share.",
                    q(s),
                    q(Selection::Claim(claim))
                ),
                s,
            )
        }
        L::AttackTreeUnlinked { tree } => {
            let s = Selection::Tree(tree);
            (
                format!("Attack tree {} is not linked to a threat scenario.", q(s)),
                s,
            )
        }
        L::AttackTreeGateEmpty { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Gate \"{}\" in attack tree {} has no children.",
                    node_label(project, tree, node),
                    q(s)
                ),
                s,
            )
        }
        L::LibraryRatingNotReviewed { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in attack tree {} has a rating copied from the library that nobody has reviewed.",
                    node_label(project, tree, node),
                    q(s)
                ),
                s,
            )
        }
        L::AttackTreeStepUnrated { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in attack tree {} is not rated.",
                    node_label(project, tree, node),
                    q(s)
                ),
                s,
            )
        }
        L::AttackTreeStepIncompleteFactors { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in attack tree {} has some attack potential factors but not all.",
                    node_label(project, tree, node),
                    q(s)
                ),
                s,
            )
        }
        L::RedundantStepFeasibility { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in attack tree {} has a direct rating that its attack potential overrides.",
                    node_label(project, tree, node),
                    q(s)
                ),
                s,
            )
        }
        L::SecurityControlUnused { control } => {
            let s = Selection::Control(control);
            (
                format!("Control {} is not attached to any attack step.", q(s)),
                s,
            )
        }
        L::CybersecurityGoalWithoutControls { goal } => {
            let s = Selection::Goal(goal);
            (format!("Cybersecurity goal {} has no controls.", q(s)), s)
        }
        L::CybersecurityGoalUnlinked { goal } => {
            let s = Selection::Goal(goal);
            (
                format!("No threat scenario uses cybersecurity goal {}.", q(s)),
                s,
            )
        }
        L::CybersecurityClaimUnlinked { claim } => {
            let s = Selection::Claim(claim);
            (
                format!("No threat scenario uses cybersecurity claim {}.", q(s)),
                s,
            )
        }
    };
    ProblemRow {
        error: code.severity() == LintSeverity::Error,
        text,
        target: Some(target),
    }
}

/// Every problem, errors first. Time O(lint).
#[must_use]
pub fn problems(ws: &Workspace) -> Vec<ProblemRow> {
    let project = ws.project();
    let mut rows: Vec<ProblemRow> = project
        .lint()
        .iter()
        .map(|f| problem(project, f.code()))
        .collect();
    rows.sort_by_key(|r| !r.error);
    rows
}

// ---------------------------------------------------------------------------
// Overview.
// ---------------------------------------------------------------------------

/// A count with a label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Count {
    /// What is counted.
    pub label: String,
    /// How many.
    pub count: usize,
    /// Color bucket (risk distribution only).
    pub tone: Tone,
}

/// The project at a glance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overview {
    /// Entities by kind.
    pub counts: Vec<Count>,
    /// Threats per inherent risk level, highest first, then "not rated".
    pub inherent: Vec<Count>,
    /// Threats per residual risk level.
    pub residual: Vec<Count>,
    /// Problems that block (errors).
    pub errors: usize,
    /// Problems that do not block.
    pub warnings: usize,
    /// The five highest risks.
    pub top: Vec<RiskRow>,
}

fn distribution(ws: &Workspace, residual: bool) -> Vec<Count> {
    let project = ws.project();
    let m = project.methodology();
    let levels: Vec<(aegis_model::RiskLevel, String)> = m
        .risk_levels()
        .map(|(l, _, label)| (l, label.to_owned()))
        .collect();
    let mut counts = vec![0usize; levels.len() + 1];
    for (_, id, _) in project.threat_scenarios() {
        let r = if residual {
            ws.cache().evaluate_residual_risk(project, id)
        } else {
            ws.cache().evaluate_risk(project, id)
        };
        let slot = r
            .ok()
            .and_then(aegis_model::ThreatRisk::max_risk)
            .and_then(|l| levels.iter().position(|(x, _)| *x == l))
            .unwrap_or(levels.len());
        counts[slot] += 1;
    }
    let n = levels.len();
    let mut out: Vec<Count> = levels
        .into_iter()
        .enumerate()
        .rev()
        .map(|(i, (_, label))| Count {
            label,
            count: counts[i],
            tone: Tone::from_level(i, n),
        })
        .collect();
    out.push(Count {
        label: "Not rated".to_owned(),
        count: counts[n],
        tone: Tone::Unrated,
    });
    out
}

/// The overview. Time O(threats log threats + lint).
#[must_use]
pub fn overview(ws: &Workspace) -> Overview {
    let p = ws.project();
    let c = |label: &str, count| Count {
        label: label.to_owned(),
        count,
        tone: Tone::Unrated,
    };
    let lint = p.lint();
    let errors = lint
        .iter()
        .filter(|f| f.severity() == LintSeverity::Error)
        .count();
    let mut top = risk_rows(ws);
    top.truncate(5);
    Overview {
        counts: vec![
            c("Assets", p.asset_count()),
            c("Damage scenarios", p.damage_scenario_count()),
            c("Threat scenarios", p.threat_scenario_count()),
            c("Attack trees", p.attack_tree_count()),
            c("Security controls", p.security_control_count()),
            c("Cybersecurity goals", p.cybersecurity_goal_count()),
            c("Cybersecurity claims", p.cybersecurity_claim_count()),
        ],
        inherent: distribution(ws, false),
        residual: distribution(ws, true),
        errors,
        warnings: lint.len() - errors,
        top,
    }
}

/// The page each selection belongs to.
#[must_use]
pub fn page_of(s: Selection) -> crate::view::Page {
    use crate::view::Page;
    match s {
        Selection::Asset(_) => Page::Assets,
        Selection::Damage(_) => Page::Damage,
        Selection::Threat(_) => Page::Threats,
        Selection::Tree(_) | Selection::TreeNode(_, _) => Page::Trees,
        Selection::Control(_) => Page::Controls,
        Selection::Goal(_) | Selection::Claim(_) => Page::Goals,
    }
}

/// Applies a selection to the UI state.
pub fn select(ui: &mut UiState, s: Selection) {
    match s {
        Selection::Asset(id) => ui.selected_asset = Some(id),
        Selection::Damage(id) => ui.selected_damage = Some(id),
        Selection::Threat(id) => {
            ui.selected_threat = Some(id);
        }
        Selection::Tree(id) => {
            if ui.selected_tree != Some(id) {
                ui.selected_node = None;
            }
            ui.selected_tree = Some(id);
        }
        Selection::TreeNode(tree, node) => {
            ui.selected_tree = Some(tree);
            ui.selected_node = Some(node);
        }
        Selection::Control(id) => ui.selected_control = Some(id),
        Selection::Goal(id) => ui.selected_goal = Some(id),
        Selection::Claim(id) => ui.selected_claim = Some(id),
    }
}

// ---------------------------------------------------------------------------
// Versions: comparison with a baseline.
// ---------------------------------------------------------------------------

/// One line of a comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    /// `Added`, `Removed`, `Changed` or `Risk`.
    pub kind: String,
    /// What, in words.
    pub text: String,
}

/// The current state compared with a baseline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// The baseline's name.
    pub baseline: String,
    /// One sentence.
    pub summary: String,
    /// The changes, entity kind by kind, then the risks that moved.
    pub lines: Vec<DiffLine>,
}

fn risk_snapshot_text(m: &Methodology, s: Option<aegis_model::RiskSnapshot>) -> String {
    match s {
        None => "not evaluated".to_owned(),
        Some(s) => match s.max_risk() {
            Some(r) => m.risk_level_label(r).unwrap_or("?").to_owned(),
            None => format!("feasibility {}", feasibility_label(m, s.feasibility())),
        },
    }
}

/// Compares `base` (a baseline) with `current`. Time proportional to the
/// change (rule A7, `Project::diff`) plus the lines written.
///
/// # Errors
///
/// The model's error if the two use different methodologies.
pub fn comparison(
    base: &Project,
    baseline: &str,
    current: &Project,
) -> Result<Comparison, aegis_model::ModelError> {
    let diff = base.diff(current)?;
    let m = current.methodology();
    let mut lines = Vec::new();
    let mut push = |kind: &str, text: String| {
        lines.push(DiffLine {
            kind: kind.to_owned(),
            text,
        });
    };
    macro_rules! entity {
        ($what:literal, $set:expr, $name:expr) => {{
            let set = $set;
            for id in set.added() {
                push("Added", format!("{} \"{}\"", $what, $name(current, *id)));
            }
            for id in set.removed() {
                push("Removed", format!("{} \"{}\"", $what, $name(base, *id)));
            }
            for id in set.modified() {
                push("Changed", format!("{} \"{}\"", $what, $name(current, *id)));
            }
        }};
    }
    entity!("Asset", diff.assets(), |p: &Project, id| named(
        p,
        Selection::Asset(id)
    )
    .to_owned());
    entity!(
        "Damage scenario",
        diff.damage_scenarios(),
        |p: &Project, id| named(p, Selection::Damage(id)).to_owned()
    );
    entity!(
        "Threat scenario",
        diff.threat_scenarios(),
        |p: &Project, id| named(p, Selection::Threat(id)).to_owned()
    );
    entity!("Attack tree", diff.attack_trees(), |p: &Project, id| named(
        p,
        Selection::Tree(id)
    )
    .to_owned());
    entity!(
        "Security control",
        diff.security_controls(),
        |p: &Project, id| named(p, Selection::Control(id)).to_owned()
    );
    entity!("Goal", diff.cybersecurity_goals(), |p: &Project, id| named(
        p,
        Selection::Goal(id)
    )
    .to_owned());
    entity!("Claim", diff.cybersecurity_claims(), |p: &Project, id| {
        named(p, Selection::Claim(id)).to_owned()
    });
    let mut moved = 0;
    for d in diff.changed_risks() {
        moved += 1;
        let name = if current.threat_scenario_idx(&d.threat()).is_some() {
            named(current, Selection::Threat(d.threat()))
        } else {
            named(base, Selection::Threat(d.threat()))
        };
        push(
            "Risk",
            format!(
                "\"{name}\": before controls {} to {} ({}); after controls {} to {} ({})",
                risk_snapshot_text(m, d.inherent().before()),
                risk_snapshot_text(m, d.inherent().after()),
                d.inherent().change().as_str(),
                risk_snapshot_text(m, d.residual().before()),
                risk_snapshot_text(m, d.residual().after()),
                d.residual().change().as_str(),
            ),
        );
    }
    let count = |k: &str| lines.iter().filter(|l| l.kind == k).count();
    let summary = if lines.is_empty() {
        "Nothing changed since this baseline.".to_owned()
    } else {
        format!(
            "Since this baseline: {} added, {} removed, {} changed, {} risk(s) moved.",
            count("Added"),
            count("Removed"),
            count("Changed"),
            moved
        )
    };
    Ok(Comparison {
        baseline: baseline.to_owned(),
        summary,
        lines,
    })
}
