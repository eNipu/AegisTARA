//! Headless view model for [`crate::view::Page::Assumptions`] (job 136).

use std::collections::BTreeSet;

use aegis_model::{Applicability, AssumptionId, ElementRef, Project, StepRef, TreeId};

use crate::configuration::{ExcludedCounts, excluded_counts};
use crate::exclusion::reasons_text;
use crate::i18n::Catalog;
use crate::pages::selection_visual_id;
use crate::relations::{self, RelationKey};
use crate::table::selection_label;
use crate::view::{Selection, UiState};
use crate::workspace::Workspace;

/// A linked claim or condition displayed on an assumption detail panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssumptionLink {
    pub id: String,
    pub visual_id: String,
    pub name: String,
    pub selection: Selection,
}

/// An attack tree or step excluded when an assumption holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssumptionExclude {
    pub target: Selection,
    pub visual_id: String,
    pub label: String,
    pub is_tree: bool,
}

/// One assumption row on the Assumptions page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssumptionRow {
    pub id: AssumptionId,
    pub visual_id: String,
    pub name: String,
    pub holds: bool,
    pub claims: usize,
    pub excludes: usize,
    pub excluded: bool,
    pub excluded_reason: Option<String>,
}

/// Detail model for the currently selected assumption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssumptionDetail {
    pub id: AssumptionId,
    pub visual_id: String,
    pub name: String,
    pub description: String,
    pub holds: bool,
    pub excluded: bool,
    pub excluded_reason: Option<String>,
    pub claims: Vec<AssumptionLink>,
    pub conditions: Vec<AssumptionLink>,
    pub excludes: Vec<AssumptionExclude>,
    pub affects_count: usize,
    pub can_create_claim: bool,
}

/// Headless view model for [`crate::view::Page::Assumptions`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssumptionsView {
    pub rows: Vec<AssumptionRow>,
    pub selected: Option<AssumptionDetail>,
    pub explainer: String,
    pub excluded: ExcludedCounts,
}

fn to_links(project: &Project, items: Vec<Selection>) -> Vec<AssumptionLink> {
    items
        .into_iter()
        .filter_map(|selection| {
            let id = match selection {
                Selection::Claim(cid) => cid.to_string(),
                Selection::Condition(cid) => cid.to_string(),
                _ => return None,
            };
            Some(AssumptionLink {
                id,
                visual_id: selection_visual_id(project, selection),
                name: selection_label(project, selection),
                selection,
            })
        })
        .collect()
}

fn exclusion_of(
    project: &Project,
    app: &Applicability,
    id: AssumptionId,
) -> (bool, Option<String>) {
    let excluded = app.is_excluded(ElementRef::Assumption(id));
    let reason = excluded
        .then(|| reasons_text(project, app.reasons(ElementRef::Assumption(id))))
        .filter(|s| !s.is_empty());
    (excluded, reason)
}

fn detail(project: &Project, app: &Applicability, id: AssumptionId) -> Option<AssumptionDetail> {
    let idx = project.assumption_idx(&id)?;
    let a = project.assumption(idx)?;
    let (excluded, excluded_reason) = exclusion_of(project, app, id);
    let sel = Selection::Assumption(id);
    let claims = to_links(
        project,
        relations::reverse_related(project, RelationKey::CLAIM_ASSUMPTIONS, sel),
    );
    let conditions = to_links(
        project,
        relations::related(project, RelationKey::ASSUMPTION_CONDITIONS, sel),
    );
    let push_ex = |target, is_tree| AssumptionExclude {
        target,
        visual_id: selection_visual_id(project, target),
        label: selection_label(project, target),
        is_tree,
    };
    let mut excludes = Vec::new();
    let mut excluded_trees = BTreeSet::new();
    for &t_idx in a.excludes_trees() {
        if let Some(tid) = project.attack_tree_id(t_idx) {
            excluded_trees.insert(tid);
            excludes.push(push_ex(Selection::Tree(tid), true));
        }
    }
    let mut direct_steps: BTreeSet<StepRef> = BTreeSet::new();
    for &sr in a.excludes_steps() {
        direct_steps.insert(sr);
        excludes.push(push_ex(Selection::TreeNode(sr.tree, sr.node), false));
    }
    let mut touched: BTreeSet<TreeId> = excluded_trees.clone();
    touched.extend(direct_steps.iter().map(|s| s.tree));
    let affected_threats = project
        .threat_scenarios()
        .filter(|(_, _, t)| {
            t.attack_tree()
                .and_then(|ti| project.attack_tree_id(ti))
                .is_some_and(|tid| touched.contains(&tid))
        })
        .count();

    Some(AssumptionDetail {
        id,
        visual_id: project.assumption_visual_id(idx).unwrap_or_default(),
        name: a.name().to_owned(),
        description: project
            .description(ElementRef::Assumption(id))
            .unwrap_or("")
            .to_owned(),
        holds: a.holds(),
        excluded,
        excluded_reason,
        claims,
        conditions,
        excludes,
        affects_count: excluded_trees.len() + direct_steps.len() + affected_threats,
        can_create_claim: true,
    })
}

/// Builds the Assumptions page view model for `ws` and `ui` in `catalog`'s language.
#[must_use]
pub fn assumptions_view(ws: &Workspace, ui: &UiState, catalog: &Catalog) -> AssumptionsView {
    let project = ws.project();
    let app = project.applicability();
    let rows = project
        .assumptions()
        .map(|(idx, id, a)| {
            let (excluded, excluded_reason) = exclusion_of(project, &app, id);
            let target = Selection::Assumption(id);
            let claims =
                relations::reverse_related(project, RelationKey::CLAIM_ASSUMPTIONS, target).len();
            AssumptionRow {
                id,
                visual_id: project.assumption_visual_id(idx).unwrap_or_default(),
                name: a.name().to_owned(),
                holds: a.holds(),
                claims,
                excludes: a.excludes_trees().len() + a.excludes_steps().len(),
                excluded,
                excluded_reason,
            }
        })
        .collect();
    let selected = ui
        .selected_assumption
        .and_then(|id| detail(project, &app, id));
    let excluded = excluded_counts(project);
    let total = excluded.elements + excluded.steps;
    let explainer = if total == 0 {
        catalog.tr("assumptions-explainer-none")
    } else {
        catalog.tr_with("assumptions-explainer", &[("n", &total.to_string())])
    };
    AssumptionsView {
        rows,
        selected,
        explainer,
        excluded,
    }
}
