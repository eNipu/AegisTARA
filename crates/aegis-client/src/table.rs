//! Table cells built from the column specs (UI-14). No UI toolkit.
//!
//! A renderer asks [`TableId::columns`] what a table shows and
//! [`table_rows`] for the cells, so no table has its own column code.

pub use aegis_canvas::Tone;

use aegis_model::{Applicability, ElementRef, Project, SystemSet, SystemUse, TreeRole};

use crate::explorer::{Folder, Owner};
use crate::i18n::Catalog;
use crate::pages::{self, Vocabulary};
use crate::relations;
use crate::view::{ColumnKind, RiskView, Selection, TableId, threat_rows};
use crate::workspace::Workspace;

/// One clickable link chip in a `ColumnKind::Links` cell (UI-14.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkChip {
    /// Linked element opened when the chip is activated.
    pub target: Selection,
    /// Short visual ID (`C00`, `A01`, ...).
    pub visual_id: String,
    /// Linked element's display name.
    pub label: String,
}

/// One cell: its text, optional risk color bucket, and link chips.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// What the cell shows.
    pub text: String,
    /// The color bucket of a risk level.
    pub tone: Option<Tone>,
    /// Clickable link chips for `ColumnKind::Links` cells; empty otherwise.
    pub chips: Vec<LinkChip>,
}

/// One row: the element it opens, one cell per column of the table
/// (in [`TableId::columns`] order, hidden columns included), and exclusion state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow {
    /// The element the row stands for; `None` for a problem with no target.
    pub target: Option<Selection>,
    /// One cell per column.
    pub cells: Vec<Cell>,
    /// Whether this row's element is excluded by an off condition (UI-14.3).
    pub excluded: bool,
    /// Human-readable reason when [`Self::excluded`] is true.
    pub excluded_reason: Option<String>,
}

/// Stable lowercase key naming one table in settings and filters.
#[must_use]
pub fn table_key(table: TableId) -> &'static str {
    match table {
        TableId::Assets => "assets",
        TableId::Damage => "damage",
        TableId::Threats => "threats",
        TableId::Risks => "risks",
        TableId::Controls => "controls",
        TableId::Goals => "goals",
        TableId::Claims => "claims",
        TableId::Problems => "problems",
        TableId::SharedSteps => "shared-steps",
        TableId::Fragments => "fragments",
    }
}

/// The table opened by an explorer [`Folder`], or `None` for non-table folders.
#[must_use]
pub fn table_for_folder(folder: Folder) -> Option<TableId> {
    match folder {
        Folder::Assets => Some(TableId::Assets),
        Folder::Damage => Some(TableId::Damage),
        Folder::Threats => Some(TableId::Threats),
        Folder::Risks => Some(TableId::Risks),
        Folder::Controls => Some(TableId::Controls),
        Folder::Goals => Some(TableId::Goals),
        Folder::Claims => Some(TableId::Claims),
        Folder::Problems => Some(TableId::Problems),
        Folder::SharedSteps => Some(TableId::SharedSteps),
        Folder::Fragments => Some(TableId::Fragments),
        _ => None,
    }
}

/// Read-only workspace JSON source for `table` (WBX-07).
#[must_use]
pub fn table_source_json(project: &Project, table: TableId) -> String {
    let (path, kind) = match table {
        TableId::Assets => ("systems/main/assets.json", "assets"),
        TableId::Damage => ("shared/damage.json", "damage"),
        TableId::Threats | TableId::Risks => ("systems/main/threats.json", "threats"),
        TableId::Controls => ("shared/controls.json", "controls"),
        TableId::Goals => ("shared/goals.json", "goals"),
        TableId::Claims => ("shared/claims.json", "claims"),
        TableId::SharedSteps => ("shared/steps.json", "shared-steps"),
        TableId::Fragments => {
            let entries: Vec<String> = project
                .to_workspace_files()
                .ok()
                .into_iter()
                .flatten()
                .filter(|f| {
                    f.path
                        .rsplit_once("/trees/")
                        .map(|(_, name)| name)
                        .or_else(|| f.path.strip_prefix("trees/"))
                        .is_some_and(|name| name.starts_with("tt") && name.ends_with(".json"))
                })
                .map(|f| String::from_utf8_lossy(&f.bytes).trim_end().to_owned())
                .collect();
            return if entries.is_empty() {
                "[]\n".to_owned()
            } else {
                format!("[\n{}\n]\n", entries.join(",\n"))
            };
        }
        TableId::Problems => return "{}\n".to_owned(),
    };
    if let Ok(files) = project.to_workspace_files()
        && let Some(file) = files.into_iter().find(|f| f.path == path)
    {
        return String::from_utf8_lossy(&file.bytes).into_owned();
    }
    let field = if kind == "shared-steps" {
        "shared_steps"
    } else {
        "items"
    };
    format!(
        "{{\n  \"format\": \"aegis-workspace\",\n  \"version\": 1,\n  \"kind\": \"{kind}\",\n  \"{field}\": []\n}}\n"
    )
}

fn text(text: impl Into<String>) -> Option<Cell> {
    Some(Cell {
        text: text.into(),
        tone: None,
        chips: Vec::new(),
    })
}

fn count(n: usize) -> Option<Cell> {
    text(n.to_string())
}

/// A risk level; `unrated` is the text of a missing one.
fn risk(r: Option<&RiskView>, unrated: String) -> Option<Cell> {
    Some(r.map_or(
        Cell {
            text: unrated,
            tone: None,
            chips: Vec::new(),
        },
        |r| Cell {
            text: r.label.clone(),
            tone: Some(r.tone),
            chips: Vec::new(),
        },
    ))
}

macro_rules! elem_name {
    ($p:expr, $id:expr, $idx:ident, $get:ident) => {
        $p.$idx(&$id).and_then(|i| $p.$get(i)).map(|e| e.name())
    };
}

/// Human-readable display name of `sel` in `p`, or empty if stale. Time O(1).
#[must_use]
pub fn selection_label(p: &Project, sel: Selection) -> String {
    use Selection as S;
    match sel {
        S::Component(id) => elem_name!(p, id, component_idx, component),
        S::Interface(id) => elem_name!(p, id, interface_idx, interface),
        S::DataFlow(id) => elem_name!(p, id, data_flow_idx, data_flow),
        S::TrustBoundary(id) => elem_name!(p, id, trust_boundary_idx, trust_boundary),
        S::Asset(id) => elem_name!(p, id, asset_idx, asset),
        S::Damage(id) => elem_name!(p, id, damage_scenario_idx, damage_scenario),
        S::Threat(id) => elem_name!(p, id, threat_scenario_idx, threat_scenario),
        S::Control(id) => elem_name!(p, id, security_control_idx, security_control),
        S::Goal(id) => elem_name!(p, id, cybersecurity_goal_idx, cybersecurity_goal),
        S::Claim(id) => elem_name!(p, id, cybersecurity_claim_idx, cybersecurity_claim),
        S::Party(id) => elem_name!(p, id, party_idx, party),
        S::Condition(id) => elem_name!(p, id, condition_idx, condition),
        S::Assumption(id) => elem_name!(p, id, assumption_idx, assumption),
        S::SharedStep(id) => p
            .shared_step_idx(&id)
            .and_then(|i| p.shared_step(i))
            .map(aegis_model::SharedStep::name),
        S::Tree(id) => p
            .attack_tree_idx(&id)
            .and_then(|i| p.attack_tree(i))
            .and_then(|t| t.get(t.root()))
            .map(aegis_model::Node::label),
        S::TreeNode(t, n) => p
            .attack_tree_idx(&t)
            .and_then(|i| p.attack_tree(i))
            .and_then(|tr| tr.idx_of(&n).and_then(|ni| tr.get(ni)))
            .map(aegis_model::Node::label),
    }
    .unwrap_or_default()
    .to_owned()
}

pub(crate) fn selection_element_ref(sel: Selection) -> Option<ElementRef> {
    use ElementRef as E;
    use Selection as S;
    Some(match sel {
        S::Asset(id) => E::Asset(id),
        S::Damage(id) => E::Damage(id),
        S::Threat(id) => E::Threat(id),
        S::Tree(id) => E::Tree(id),
        S::Control(id) => E::Control(id),
        S::Goal(id) => E::Goal(id),
        S::Claim(id) => E::Claim(id),
        S::Assumption(id) => E::Assumption(id),
        S::SharedStep(id) => E::SharedStep(id),
        S::Component(id) => E::Component(id),
        S::Interface(id) => E::Interface(id),
        S::DataFlow(id) => E::DataFlow(id),
        S::TrustBoundary(id) => E::TrustBoundary(id),
        S::TreeNode(_, _) | S::Party(_) | S::Condition(_) => return None,
    })
}

fn exclusion_state(
    p: &Project,
    app: &Applicability,
    target: Option<Selection>,
) -> (bool, Option<String>) {
    let reasons = match target {
        Some(Selection::TreeNode(tree, node)) if app.is_node_excluded(tree, node) => {
            app.node_reasons(tree, node)
        }
        Some(sel) => match selection_element_ref(sel).filter(|&e| app.is_excluded(e)) {
            Some(elem) => app.reasons(elem),
            None => return (false, None),
        },
        None => return (false, None),
    };
    (true, Some(crate::exclusion::reasons_text(p, reasons)))
}

/// Whether `sel` is excluded by an off condition in `p`, with its human-readable
/// reason when excluded. Time O(project applicability).
#[must_use]
pub fn selection_exclusion(p: &Project, sel: Selection) -> (bool, Option<String>) {
    exclusion_state(p, &p.applicability(), Some(sel))
}

pub(crate) fn selection_system_set(
    p: &Project,
    u: &SystemUse,
    sel: Selection,
) -> Option<SystemSet> {
    use Selection as S;
    match sel {
        S::Component(id) => p.component_idx(&id).map(|i| u.component(i)),
        S::Interface(id) => p.interface_idx(&id).map(|i| u.interface(i)),
        S::DataFlow(id) => p.data_flow_idx(&id).map(|i| u.data_flow(i)),
        S::TrustBoundary(id) => p.trust_boundary_idx(&id).map(|i| u.trust_boundary(i)),
        S::Asset(id) => p.asset_idx(&id).map(|i| u.asset(i)),
        S::Damage(id) => p.damage_scenario_idx(&id).map(|i| u.damage(i)),
        S::Threat(id) => p.threat_scenario_idx(&id).map(|i| u.threat(i)),
        S::Tree(id) | S::TreeNode(id, _) => p.attack_tree_idx(&id).map(|i| u.tree(i)),
        S::Control(id) => p.security_control_idx(&id).map(|i| u.control(i)),
        S::Goal(id) => p.cybersecurity_goal_idx(&id).map(|i| u.goal(i)),
        S::Claim(id) => p.cybersecurity_claim_idx(&id).map(|i| u.claim(i)),
        S::SharedStep(id) => p.shared_step_idx(&id).map(|i| u.shared_step(i)),
        S::Party(_) | S::Condition(_) | S::Assumption(_) => None,
    }
}

fn matches_owner(
    p: &Project,
    u: &SystemUse,
    sys_set: Option<SystemSet>,
    target: Option<Selection>,
) -> bool {
    use Selection as S;
    let Some((sel, set)) = target.and_then(|s| selection_system_set(p, u, s).map(|m| (s, m)))
    else {
        return false;
    };
    match sys_set {
        Some(s) => set.intersects(s),
        None => {
            set.is_empty()
                && !matches!(
                    sel,
                    S::Damage(_) | S::Control(_) | S::Goal(_) | S::Claim(_) | S::SharedStep(_)
                )
        }
    }
}

/// Builds the rows of `table` from `items`. A column `cell` does not know
/// shows as `[key]`, like a missing catalog id, so a gap is visible.
fn build<R>(
    table: TableId,
    items: &[R],
    target: impl Fn(&R) -> Option<Selection>,
    cell: impl Fn(&R, &str) -> Option<Cell>,
) -> Vec<TableRow> {
    let columns = table.columns();
    items
        .iter()
        .map(|r| TableRow {
            target: target(r),
            cells: columns
                .iter()
                .map(|c| cell(r, c.key).unwrap_or_else(|| Cell::from_key(c.key)))
                .collect(),
            excluded: false,
            excluded_reason: None,
        })
        .collect()
}

impl Cell {
    fn from_key(key: &str) -> Cell {
        Cell {
            text: format!("[{key}]"),
            tone: None,
            chips: Vec::new(),
        }
    }
}

/// The rows of `table`, in the order its page lists them. `vocab` is the
/// pack's vocabulary in the wanted language. Time: that of the table's row
/// function in [`crate::pages`] plus O(rows x columns).
#[must_use]
pub fn table_rows(
    ws: &Workspace,
    table: TableId,
    vocab: &Vocabulary,
    catalog: &Catalog,
) -> Vec<TableRow> {
    table_rows_for_owner(ws, table, Owner::Project, vocab, catalog)
}

/// The rows of `table` scoped to an explorer [`Owner`], with link chips and
/// applicability exclusion state populated.
#[must_use]
pub fn table_rows_for_owner(
    ws: &Workspace,
    table: TableId,
    owner: Owner,
    vocab: &Vocabulary,
    catalog: &Catalog,
) -> Vec<TableRow> {
    let unrated = || catalog.tr("not-rated");
    let mut rows = match table {
        TableId::Assets => build(
            table,
            &pages::asset_rows(ws),
            |r| Some(Selection::Asset(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.name),
                "component" => text(&r.component),
                "properties" => text(
                    r.properties
                        .iter()
                        .zip(&vocab.properties)
                        .filter(|(on, _)| **on)
                        .map(|(_, label)| label.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                "threats" => count(r.threats),
                "worst-risk" => risk(r.worst.as_ref(), String::new()),
                "conditions" => text(String::new()),
                _ => None,
            },
        ),
        TableId::Damage => build(
            table,
            &pages::damage_rows(ws),
            |r| Some(Selection::Damage(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.name),
                "impact" => text(&r.impacts_text),
                "threats" => count(r.threats),
                _ => None,
            },
        ),
        TableId::Threats => build(
            table,
            &threat_rows(ws),
            |r| Some(Selection::Threat(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.name),
                "asset" => text(&r.asset),
                "scales" => text(match r.scales {
                    Some(true) => catalog.tr("yes"),
                    Some(false) => catalog.tr("no"),
                    None => String::new(),
                }),
                "before" => risk(r.inherent.as_ref(), unrated()),
                "after" => risk(r.residual.as_ref(), unrated()),
                "conditions" => text(String::new()),
                _ => None,
            },
        ),
        TableId::Risks => build(
            table,
            &pages::risk_rows(ws),
            |r| Some(Selection::Threat(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.threat),
                "asset" => text(&r.asset),
                "damage" => text(&r.damage),
                "impact" => text(&r.impact),
                "feasibility" => text(&r.feasibility),
                "rationale" => text(&r.rationale),
                "before" => risk(r.inherent.as_ref(), unrated()),
                "after" => risk(r.residual.as_ref(), unrated()),
                "treatment" => text(&r.treatment),
                "backing" => text(&r.backing),
                "reduced-by" => text(pages::reduced_by_cell(r)),
                _ => None,
            },
        ),
        TableId::Controls => build(
            table,
            &pages::control_rows(ws),
            |r| Some(Selection::Control(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.name),
                "steps" => count(r.steps),
                "goals" => count(r.goals),
                "lowers" => count(r.lowers),
                _ => None,
            },
        ),
        TableId::Goals => build(
            table,
            &pages::goal_rows(ws),
            |r| Some(Selection::Goal(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.name),
                "responsible" => text(r.responsible.join(", ")),
                "controls" => count(r.controls),
                "threats" => count(r.threats),
                "conditions" => text(String::new()),
                _ => None,
            },
        ),
        TableId::Claims => build(
            table,
            &pages::claim_rows(ws),
            |r| Some(Selection::Claim(r.id)),
            |r, key| match key {
                "id" => text(&r.visual_id),
                "name" => text(&r.name),
                "responsible" => text(r.responsible.join(", ")),
                "threats" => count(r.threats),
                "assumptions" => text(String::new()),
                "conditions" => text(String::new()),
                _ => None,
            },
        ),
        TableId::Problems => build(
            table,
            &pages::problems(ws),
            |r| r.target,
            |r, key| match key {
                "id" => text(&r.visual_id),
                "severity" => text(catalog.tr(if r.error { "blocking" } else { "warning" })),
                "problem" => text(&r.text),
                _ => None,
            },
        ),
        TableId::SharedSteps => {
            let project = ws.project();
            let m = project.methodology();
            let shared: Vec<_> = project.shared_steps().collect();
            let feasibility = |level: Option<aegis_model::FeasibilityLevel>| {
                level
                    .and_then(|level| m.feasibility_levels().position(|(l, _, _)| l == level))
                    .and_then(|i| vocab.feasibility.get(i))
                    .cloned()
                    .unwrap_or_else(unrated)
            };
            let attack_type = |attack: Option<aegis_model::AttackType>| {
                attack
                    .and_then(|attack| m.attack_types().position(|a| a == attack))
                    .and_then(|i| vocab.attack_types.get(i))
                    .cloned()
                    .unwrap_or_else(unrated)
            };
            build(
                table,
                &shared,
                |(_, id, _)| Some(Selection::SharedStep(*id)),
                |(idx, _, step), key| match key {
                    "id" => text(project.shared_step_visual_id(*idx).unwrap_or_default()),
                    "name" => text(step.name()),
                    "feasibility" => text(feasibility(step.feasibility())),
                    "via" | "used-in" => text(String::new()),
                    "attack-type" => text(attack_type(step.attack_type())),
                    _ => None,
                },
            )
        }
        TableId::Fragments => {
            if owner != Owner::Project {
                return Vec::new();
            }
            let project = ws.project();
            let m = project.methodology();
            let fragments: Vec<_> = project
                .attack_trees()
                .filter(|(idx, _, _)| project.attack_tree_role(*idx) == Some(TreeRole::Fragment))
                .collect();
            let feasibility = |id, tree: &aegis_model::AttackTree| {
                ws.cache()
                    .evaluate_tree(project, id)
                    .ok()
                    .and_then(|eval| eval.get(tree.root()).copied())
                    .and_then(|level| m.feasibility_levels().position(|(l, _, _)| l == level))
                    .and_then(|i| vocab.feasibility.get(i))
                    .cloned()
                    .unwrap_or_else(unrated)
            };
            build(
                table,
                &fragments,
                |(_, id, _)| Some(Selection::Tree(*id)),
                |(idx, id, tree), key| match key {
                    "id" => text(project.attack_tree_visual_id(*idx).unwrap_or_default()),
                    "name" => text(
                        tree.get(tree.root())
                            .map(aegis_model::Node::label)
                            .unwrap_or_default(),
                    ),
                    "feasibility" => text(feasibility(*id, tree)),
                    "used-in" => text(String::new()),
                    _ => None,
                },
            )
        }
    };
    let project = ws.project();
    if owner != Owner::Project {
        let usage = project.system_use();
        let sys_set = match owner {
            Owner::System(id) => Some(usage.set_of([id])),
            Owner::Project | Owner::Unassigned => None,
        };
        rows.retain(|row| matches_owner(project, &usage, sys_set, row.target));
    }
    let app = project.applicability();
    let columns = table.columns();
    for row in &mut rows {
        if let Some(from) = row.target {
            for (col, cell) in columns.iter().zip(&mut row.cells) {
                if let ColumnKind::Links(rel) = col.kind {
                    cell.chips = relations::related(project, rel, from)
                        .into_iter()
                        .map(|target| LinkChip {
                            target,
                            visual_id: pages::selection_visual_id(project, target),
                            label: selection_label(project, target),
                        })
                        .collect();
                }
            }
        }
        (row.excluded, row.excluded_reason) = exclusion_state(project, &app, row.target);
    }
    rows
}
