//! Headless view model for the Configuration page (job 135).

use aegis_model::{ConditionId, Project};

use crate::i18n::Catalog;
use crate::relations::{self, RelationKey};
use crate::view::Selection;
use crate::workspace::Workspace;

/// The seven element-level condition relation keys.
const ELEMENT_CONDITION_KEYS: [RelationKey; 7] = [
    RelationKey::ASSET_CONDITIONS,
    RelationKey::COMPONENT_CONDITIONS,
    RelationKey::INTERFACE_CONDITIONS,
    RelationKey::THREAT_CONDITIONS,
    RelationKey::TREE_CONDITIONS,
    RelationKey::GOAL_CONDITIONS,
    RelationKey::CLAIM_CONDITIONS,
];

/// One condition row on the Configuration page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationRow {
    /// Stable condition ID.
    pub id: ConditionId,
    /// Short visual ID (`CF00`, `CF01`, ...).
    pub visual_id: String,
    /// Condition name.
    pub name: String,
    /// Plain-text description.
    pub description: String,
    /// Whether the condition is currently present.
    pub present: bool,
    /// Number of elements (assets, components, interfaces, threats, trees,
    /// goals, claims) that need this condition.
    pub needed_by_elements: usize,
    /// Number of attack-tree steps that need this condition.
    pub needed_by_steps: usize,
}

/// Counts of elements and attack-tree leaf steps currently excluded by
/// applicability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExcludedCounts {
    /// Excluded elements from [`aegis_model::Applicability::excluded_elements`].
    pub elements: usize,
    /// Excluded leaf steps from [`aegis_model::Applicability::excluded_nodes`].
    pub steps: usize,
}

/// Headless view model for [`crate::view::Page::Configuration`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigurationView {
    /// Conditions in project order.
    pub rows: Vec<ConfigurationRow>,
    /// One-line summary explaining what switching conditions off does and how
    /// many items are currently excluded.
    pub explainer: String,
    /// Breakdown of currently excluded elements and leaf steps.
    pub excluded: ExcludedCounts,
}

/// Counts excluded elements and excluded leaf steps in `project`.
#[must_use]
pub fn excluded_counts(project: &Project) -> ExcludedCounts {
    let app = project.applicability();
    let elements = app.excluded_elements().count();
    let steps = app
        .excluded_nodes()
        .filter(|(tree_id, node_id, _)| {
            project
                .attack_tree_idx(tree_id)
                .and_then(|t_idx| project.attack_tree(t_idx))
                .and_then(|tree| tree.idx_of(node_id).and_then(|n_idx| tree.get(n_idx)))
                .is_some_and(|node| node.gate_kind().is_none())
        })
        .count();
    ExcludedCounts { elements, steps }
}

/// Builds the Configuration page view model for `ws` in `catalog`'s language.
#[must_use]
pub fn configuration_view(ws: &Workspace, catalog: &Catalog) -> ConfigurationView {
    let project = ws.project();
    let rows = project
        .conditions()
        .map(|(idx, id, cond)| {
            let target = Selection::Condition(id);
            let needed_by_elements = ELEMENT_CONDITION_KEYS
                .iter()
                .map(|&key| relations::reverse_related(project, key, target).len())
                .sum();
            let needed_by_steps =
                relations::reverse_related(project, RelationKey::STEP_CONDITIONS, target).len();
            ConfigurationRow {
                id,
                visual_id: project.condition_visual_id(idx).unwrap_or_default(),
                name: cond.name().to_owned(),
                description: cond.description().to_owned(),
                present: cond.present(),
                needed_by_elements,
                needed_by_steps,
            }
        })
        .collect();
    let excluded = excluded_counts(project);
    let total = excluded.elements + excluded.steps;
    let explainer = if total == 0 {
        catalog.tr("configuration-explainer-none")
    } else {
        let n = total.to_string();
        catalog.tr_with("configuration-explainer", &[("n", &n)])
    };
    ConfigurationView {
        rows,
        explainer,
        excluded,
    }
}
