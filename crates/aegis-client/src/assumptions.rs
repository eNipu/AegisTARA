//! Headless view model for [`crate::view::Page::Assumptions`] (job 136).

use aegis_model::{Applicability, AssumptionId, ElementRef, Project};

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
    pub excluded: crate::configuration::ExcludedCounts,
}

fn links(p: &Project, key: RelationKey, sel: Selection, rev: bool) -> Vec<AssumptionLink> {
    let items = if rev {
        relations::reverse_related(p, key, sel)
    } else {
        relations::related(p, key, sel)
    };
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
                visual_id: selection_visual_id(p, selection),
                name: selection_label(p, selection),
                selection,
            })
        })
        .collect()
}

fn exclusion_of(p: &Project, app: &Applicability, id: AssumptionId) -> (bool, Option<String>) {
    let excluded = app.is_excluded(ElementRef::Assumption(id));
    let reason = excluded
        .then(|| crate::exclusion::reasons_text(p, app.reasons(ElementRef::Assumption(id))))
        .filter(|s| !s.is_empty());
    (excluded, reason)
}

fn detail(p: &Project, app: &Applicability, id: AssumptionId) -> Option<AssumptionDetail> {
    let idx = p.assumption_idx(&id)?;
    let a = p.assumption(idx)?;
    let (excluded, excluded_reason) = exclusion_of(p, app, id);
    let sel = Selection::Assumption(id);
    let claims = links(p, RelationKey::CLAIM_ASSUMPTIONS, sel, true);
    let conditions = links(p, RelationKey::ASSUMPTION_CONDITIONS, sel, false);
    let push_ex = |target, is_tree| AssumptionExclude {
        target,
        visual_id: selection_visual_id(p, target),
        label: selection_label(p, target),
        is_tree,
    };
    let mut excludes = Vec::new();
    let mut trees = std::collections::BTreeSet::new();
    for &t_idx in a.excludes_trees() {
        if let Some(tid) = p.attack_tree_id(t_idx) {
            trees.insert(tid);
            excludes.push(push_ex(Selection::Tree(tid), true));
        }
    }
    let mut steps = std::collections::BTreeSet::new();
    for &sr in a.excludes_steps() {
        steps.insert(sr);
        excludes.push(push_ex(Selection::TreeNode(sr.tree, sr.node), false));
    }
    let affected_threats = p
        .threat_scenarios()
        .filter(|(_, _, t)| {
            t.attack_tree()
                .and_then(|ti| p.attack_tree_id(ti))
                .is_some_and(|tid| trees.contains(&tid) || steps.iter().any(|s| s.tree == tid))
        })
        .count();

    Some(AssumptionDetail {
        id,
        visual_id: p.assumption_visual_id(idx).unwrap_or_default(),
        name: a.name().to_owned(),
        description: p
            .description(ElementRef::Assumption(id))
            .unwrap_or("")
            .to_owned(),
        holds: a.holds(),
        excluded,
        excluded_reason,
        claims,
        conditions,
        excludes,
        affects_count: trees.len() + steps.len() + affected_threats,
        can_create_claim: true,
    })
}

/// Builds the Assumptions page view model for `ws` and `ui` in `catalog`'s language.
#[must_use]
pub fn assumptions_view(ws: &Workspace, ui: &UiState, catalog: &Catalog) -> AssumptionsView {
    let p = ws.project();
    let app = p.applicability();
    let rows = p
        .assumptions()
        .map(|(idx, id, a)| {
            let (excluded, excluded_reason) = exclusion_of(p, &app, id);
            let target = Selection::Assumption(id);
            let claims =
                relations::reverse_related(p, RelationKey::CLAIM_ASSUMPTIONS, target).len();
            AssumptionRow {
                id,
                visual_id: p.assumption_visual_id(idx).unwrap_or_default(),
                name: a.name().to_owned(),
                holds: a.holds(),
                claims,
                excludes: a.excludes_trees().len() + a.excludes_steps().len(),
                excluded,
                excluded_reason,
            }
        })
        .collect();
    let selected = ui.selected_assumption.and_then(|id| detail(p, &app, id));
    let excluded = crate::configuration::excluded_counts(p);
    let explainer = match excluded.elements + excluded.steps {
        0 => catalog.tr("assumptions-explainer-none"),
        n => catalog.tr_with("assumptions-explainer", &[("n", &n.to_string())]),
    };
    AssumptionsView {
        rows,
        selected,
        explainer,
        excluded,
    }
}
