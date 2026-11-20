//! Typed cell editor descriptors, inline cell editing, and multi-cell editing.

use aegis_model::{Project, ThreatId, ThreatScenario};

use crate::commands::{Edit, TREATMENTS};
use crate::grid::GridError;
use crate::i18n::Catalog;
use crate::pages::Vocabulary;
use crate::relations::{self, RelationKey};
use crate::view::{ColumnKind, ColumnSpec, Selection, TableId};

/// Truncates untrusted text to at most 64 Unicode scalar values before
/// placing it in a [`GridError`] message. Time O(1).
pub(crate) fn trunc64(s: &str) -> String {
    s.chars().take(64).collect()
}

/// Flattens any nested batches and strips no-op edits via [`Edit::changing`].
/// Time O(edits).
pub(crate) fn finish_batch(edits: Vec<Edit>, project: &Project) -> Edit {
    let mut flat = Vec::with_capacity(edits.len());
    for edit in edits {
        push_flat(&mut flat, edit);
    }
    Edit::Batch(flat).changing(project)
}

fn push_flat(out: &mut Vec<Edit>, edit: Edit) {
    match edit {
        Edit::Batch(items) => {
            for item in items {
                push_flat(out, item);
            }
        }
        other => out.push(other),
    }
}

/// Whether `target` is the row selection kind belonging to `table`. Time O(1).
pub(crate) fn target_matches_table(table: TableId, target: Selection) -> bool {
    matches!(
        (table, target),
        (TableId::Assets, Selection::Asset(_))
            | (TableId::Damage, Selection::Damage(_))
            | (TableId::Threats | TableId::Risks, Selection::Threat(_))
            | (TableId::Controls, Selection::Control(_))
            | (TableId::Goals, Selection::Goal(_))
            | (TableId::Claims, Selection::Claim(_))
    )
}

/// Whether the element identified by `target` currently exists in `project`.
/// Time O(1).
pub(crate) fn target_exists(project: &Project, target: Selection) -> bool {
    let p = project;
    match target {
        Selection::Component(id) => p.component_idx(&id).and_then(|i| p.component(i)).is_some(),
        Selection::Interface(id) => p.interface_idx(&id).and_then(|i| p.interface(i)).is_some(),
        Selection::DataFlow(id) => p.data_flow_idx(&id).and_then(|i| p.data_flow(i)).is_some(),
        Selection::TrustBoundary(id) => p
            .trust_boundary_idx(&id)
            .and_then(|i| p.trust_boundary(i))
            .is_some(),
        Selection::Asset(id) => p.asset_idx(&id).and_then(|i| p.asset(i)).is_some(),
        Selection::Damage(id) => p
            .damage_scenario_idx(&id)
            .and_then(|i| p.damage_scenario(i))
            .is_some(),
        Selection::Threat(id) => p
            .threat_scenario_idx(&id)
            .and_then(|i| p.threat_scenario(i))
            .is_some(),
        Selection::Tree(id) => p
            .attack_tree_idx(&id)
            .and_then(|i| p.attack_tree(i))
            .is_some(),
        Selection::TreeNode(tree_id, node_id) => p
            .attack_tree_idx(&tree_id)
            .and_then(|i| p.attack_tree(i))
            .and_then(|t| t.idx_of(&node_id))
            .is_some(),
        Selection::Control(id) => p
            .security_control_idx(&id)
            .and_then(|i| p.security_control(i))
            .is_some(),
        Selection::Goal(id) => p
            .cybersecurity_goal_idx(&id)
            .and_then(|i| p.cybersecurity_goal(i))
            .is_some(),
        Selection::Claim(id) => p
            .cybersecurity_claim_idx(&id)
            .and_then(|i| p.cybersecurity_claim(i))
            .is_some(),
        Selection::Party(id) => p.party_idx(&id).and_then(|i| p.party(i)).is_some(),
        Selection::Condition(id) => p.condition_idx(&id).and_then(|i| p.condition(i)).is_some(),
    }
}

/// Which inline editor a table cell opens, with its current value and options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellEditorKind {
    /// Cell cannot be edited in place.
    ReadOnly,
    /// Inline single-line text editor.
    Text {
        /// Current text value of the cell.
        current: String,
    },
    /// Single-choice dropdown or popover.
    SingleChoice {
        /// Display labels in choice order.
        options: Vec<String>,
        /// Currently selected index, if any.
        selected: Option<usize>,
        /// Whether the choice can be cleared to `None`.
        allow_clear: bool,
    },
    /// Multi-choice toggle list (such as asset security properties).
    MultiChoice {
        /// `(label, checked)` pairs in methodology order.
        options: Vec<(String, bool)>,
    },
    /// Per-category impact rating matrix for a damage scenario.
    ImpactRatings {
        /// `(category_label, level_labels, selected_level)` triples.
        categories: Vec<(String, Vec<String>, Option<usize>)>,
    },
    /// Link picker backed by a relation in [`crate::relations`].
    LinkPicker {
        /// Registry key of the relation to edit.
        relation: RelationKey,
    },
}

/// Typed value submitted by a cell editor or bulk cell edit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellInput {
    /// Text value for a name cell.
    Text(String),
    /// Single-choice index, or `None` to clear.
    SingleChoice(Option<usize>),
    /// Exact bit-vector of flags for asset security properties.
    MultiChoice(Vec<bool>),
    /// Single impact category rating (`level: None` clears).
    Impact {
        /// Position in the methodology's impact categories.
        category: usize,
        /// Position in that category's levels, or `None` to clear.
        level: Option<usize>,
    },
    /// Full vector of impact levels across all categories at once.
    Impacts(Vec<Option<usize>>),
}

/// Describes the editor a table cell opens for `(table, target, col)`.
///
/// Returns [`CellEditorKind::ReadOnly`] for [`TableId::Problems`], missing or
/// stale targets, mismatched selection kinds, read-only columns, and
/// non-editable link relations (such as `THREAT_ASSET`).
///
/// Time: O(properties + impact categories).
#[must_use]
pub fn cell_editor(
    project: &Project,
    table: TableId,
    target: Option<Selection>,
    col: &ColumnSpec,
    vocab: &Vocabulary,
    catalog: &Catalog,
) -> CellEditorKind {
    if table == TableId::Problems {
        return CellEditorKind::ReadOnly;
    }
    let Some(t) = target else {
        return CellEditorKind::ReadOnly;
    };
    if !target_matches_table(table, t) || !target_exists(project, t) {
        return CellEditorKind::ReadOnly;
    }
    match col.kind {
        ColumnKind::Id | ColumnKind::Computed | ColumnKind::Text => CellEditorKind::ReadOnly,
        ColumnKind::Name => element_name(project, t).map_or(CellEditorKind::ReadOnly, |current| {
            CellEditorKind::Text { current }
        }),
        ColumnKind::Links(key) => {
            if relations::relation(key).and_then(|r| r.link).is_some() {
                CellEditorKind::LinkPicker { relation: key }
            } else {
                CellEditorKind::ReadOnly
            }
        }
        ColumnKind::Rating => rating_editor(project, table, t, col.key, vocab, catalog),
    }
}

pub(crate) fn element_name(p: &Project, target: Selection) -> Option<String> {
    let name = match target {
        Selection::Asset(id) => p.asset(p.asset_idx(&id)?)?.name(),
        Selection::Damage(id) => p.damage_scenario(p.damage_scenario_idx(&id)?)?.name(),
        Selection::Threat(id) => p.threat_scenario(p.threat_scenario_idx(&id)?)?.name(),
        Selection::Control(id) => p.security_control(p.security_control_idx(&id)?)?.name(),
        Selection::Goal(id) => p.cybersecurity_goal(p.cybersecurity_goal_idx(&id)?)?.name(),
        Selection::Claim(id) => p
            .cybersecurity_claim(p.cybersecurity_claim_idx(&id)?)?
            .name(),
        _ => return None,
    };
    Some(name.to_owned())
}

fn threat<'a>(p: &'a Project, id: &ThreatId) -> Option<&'a ThreatScenario> {
    p.threat_scenario_idx(id).and_then(|i| p.threat_scenario(i))
}

fn rating_editor(
    p: &Project,
    table: TableId,
    target: Selection,
    col_key: &str,
    vocab: &Vocabulary,
    catalog: &Catalog,
) -> CellEditorKind {
    match (table, col_key, target) {
        (TableId::Assets, "properties", Selection::Asset(id)) => {
            let Some(asset) = p.asset_idx(&id).and_then(|i| p.asset(i)) else {
                return CellEditorKind::ReadOnly;
            };
            let props = asset.properties();
            let mut m_props = p.methodology().properties();
            let options = vocab
                .properties
                .iter()
                .map(|label| {
                    (
                        label.clone(),
                        m_props.next().is_some_and(|x| props.contains(x)),
                    )
                })
                .collect();
            CellEditorKind::MultiChoice { options }
        }
        (TableId::Damage, "impact", Selection::Damage(id)) => {
            let Some(damage) = p
                .damage_scenario_idx(&id)
                .and_then(|i| p.damage_scenario(i))
            else {
                return CellEditorKind::ReadOnly;
            };
            let m = p.methodology();
            let mut m_cats = m.impact_categories();
            let categories = vocab
                .impact_categories
                .iter()
                .map(|c| {
                    let selected = m_cats.next().and_then(|cat| {
                        let level = damage.impact(cat)?;
                        m.levels(cat).position(|l| l == level)
                    });
                    (c.label.clone(), c.levels.clone(), selected)
                })
                .collect();
            CellEditorKind::ImpactRatings { categories }
        }
        (TableId::Threats, "scales", Selection::Threat(id)) => {
            let Some(t) = threat(p, &id) else {
                return CellEditorKind::ReadOnly;
            };
            let selected = t.scales().map(|yes| usize::from(!yes));
            CellEditorKind::SingleChoice {
                options: vec![catalog.tr("yes"), catalog.tr("no")],
                selected,
                allow_clear: true,
            }
        }
        (TableId::Risks, "treatment", Selection::Threat(id)) => {
            let Some(t) = threat(p, &id) else {
                return CellEditorKind::ReadOnly;
            };
            let selected = t
                .treatment()
                .and_then(|tr| TREATMENTS.iter().position(|x| *x == tr));
            CellEditorKind::SingleChoice {
                options: vocab.treatments.clone(),
                selected,
                allow_clear: true,
            }
        }
        _ => CellEditorKind::ReadOnly,
    }
}

/// Builds a single [`Edit`] for editing one cell `(table, target, col)` with
/// `input`.
///
/// Time: O(impact categories + properties).
///
/// # Errors
///
/// - [`GridError::NotEditable`] if `table` is [`TableId::Problems`], `target`
///   does not match `table`, or `target` no longer exists in `project`.
/// - [`GridError::ReadOnlyColumn`] if `col` is `Id`, `Computed`, `Links(_)`,
///   `Text`, or an unrecognized `Rating` column.
/// - [`GridError::EmptyName`] (`row: 1`) if a `Name` input is blank after
///   trimming.
/// - [`GridError::UnknownRating`] (`row: 1`) if a rating choice is out of
///   bounds, has the wrong vector length, clears all asset properties, or
///   supplies the wrong [`CellInput`] variant for the column.
pub fn edit_cell(
    project: &Project,
    table: TableId,
    target: Selection,
    col: &ColumnSpec,
    input: &CellInput,
) -> Result<Edit, GridError> {
    if table == TableId::Problems
        || !target_matches_table(table, target)
        || !target_exists(project, target)
    {
        return Err(GridError::NotEditable);
    }
    match col.kind {
        ColumnKind::Id | ColumnKind::Computed | ColumnKind::Links(_) | ColumnKind::Text => {
            Err(GridError::ReadOnlyColumn)
        }
        ColumnKind::Name => edit_name_cell(target, col.key, input),
        ColumnKind::Rating => edit_rating_cell(project, table, target, col.key, input),
    }
}

fn edit_name_cell(target: Selection, col_key: &str, input: &CellInput) -> Result<Edit, GridError> {
    let CellInput::Text(raw) = input else {
        return Err(unknown_rating(col_key, ""));
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(GridError::EmptyName { row: 1 });
    }
    let name = trimmed.to_owned();
    match target {
        Selection::Asset(asset) => Ok(Edit::RenameAsset { asset, name }),
        Selection::Damage(damage) => Ok(Edit::RenameDamage { damage, name }),
        Selection::Threat(threat) => Ok(Edit::RenameThreat { threat, name }),
        Selection::Control(control) => Ok(Edit::RenameControl { control, name }),
        Selection::Goal(goal) => Ok(Edit::RenameGoal { goal, name }),
        Selection::Claim(claim) => Ok(Edit::RenameClaim { claim, name }),
        _ => Err(GridError::NotEditable),
    }
}

fn edit_rating_cell(
    project: &Project,
    table: TableId,
    target: Selection,
    col_key: &str,
    input: &CellInput,
) -> Result<Edit, GridError> {
    match (table, col_key, target) {
        (TableId::Assets, "properties", Selection::Asset(asset)) => {
            let CellInput::MultiChoice(flags) = input else {
                return Err(unknown_rating(col_key, ""));
            };
            let expected = project.methodology().properties().count();
            if flags.len() != expected {
                return Err(unknown_rating(
                    col_key,
                    &format!("{} flags (expected {expected})", flags.len()),
                ));
            }
            let properties: Vec<usize> = flags
                .iter()
                .enumerate()
                .filter_map(|(i, &on)| on.then_some(i))
                .collect();
            if properties.is_empty() {
                return Err(unknown_rating(col_key, ""));
            }
            Ok(Edit::SetAssetProperties { asset, properties })
        }
        (TableId::Damage, "impact", Selection::Damage(damage)) => {
            let m = project.methodology();
            match input {
                CellInput::Impact { category, level } => {
                    let Some(cat) = m.impact_categories().nth(*category) else {
                        return Err(unknown_rating(col_key, &category.to_string()));
                    };
                    if let Some(lvl) = *level
                        && m.levels(cat).nth(lvl).is_none()
                    {
                        return Err(unknown_rating(col_key, &lvl.to_string()));
                    }
                    Ok(Edit::SetImpact {
                        damage,
                        category: *category,
                        level: *level,
                    })
                }
                CellInput::Impacts(levels) => {
                    let expected = m.impact_category_count();
                    if levels.len() != expected {
                        return Err(unknown_rating(
                            col_key,
                            &format!("{} categories (expected {expected})", levels.len()),
                        ));
                    }
                    let mut edits = Vec::with_capacity(levels.len());
                    for (category, (cat, &level)) in
                        m.impact_categories().zip(levels.iter()).enumerate()
                    {
                        if let Some(lvl) = level
                            && m.levels(cat).nth(lvl).is_none()
                        {
                            return Err(unknown_rating(col_key, &lvl.to_string()));
                        }
                        edits.push(Edit::SetImpact {
                            damage,
                            category,
                            level,
                        });
                    }
                    Ok(finish_batch(edits, project))
                }
                _ => Err(unknown_rating(col_key, "")),
            }
        }
        (TableId::Threats, "scales", Selection::Threat(id)) => {
            let CellInput::SingleChoice(choice) = input else {
                return Err(unknown_rating(col_key, ""));
            };
            let scales = match *choice {
                None => None,
                Some(0) => Some(true),
                Some(1) => Some(false),
                Some(other) => return Err(unknown_rating(col_key, &other.to_string())),
            };
            let reason = threat(project, &id)
                .map(|t| t.scaling_reason().to_owned())
                .ok_or(GridError::NotEditable)?;
            Ok(Edit::SetThreatScaling {
                threat: id,
                scales,
                reason,
            })
        }
        (TableId::Risks, "treatment", Selection::Threat(threat)) => {
            let CellInput::SingleChoice(choice) = input else {
                return Err(unknown_rating(col_key, ""));
            };
            if let Some(ix) = *choice
                && TREATMENTS.get(ix).is_none()
            {
                return Err(unknown_rating(col_key, &ix.to_string()));
            }
            Ok(Edit::SetTreatment {
                threat,
                treatment: *choice,
            })
        }
        _ => Err(GridError::ReadOnlyColumn),
    }
}

/// Applies the same [`CellInput`] to one column across multiple `targets` in a
/// single undo step, pruning unchanged edits via [`Edit::changing`].
///
/// Time: O(targets.len()).
///
/// # Errors
///
/// - [`GridError::NotEditable`] if `targets` is empty, `table` is
///   [`TableId::Problems`], or any target is mismatched or missing in `project`.
/// - Any [`GridError`] returned by [`edit_cell`]; errors carrying a 1-based
///   `row` have `row` rewritten to the 1-based index of the failing target in
///   `targets`.
pub fn edit_cells(
    project: &Project,
    table: TableId,
    targets: &[Selection],
    col: &ColumnSpec,
    input: &CellInput,
) -> Result<Edit, GridError> {
    if targets.is_empty() {
        return Err(GridError::NotEditable);
    }
    let mut edits = Vec::with_capacity(targets.len());
    for (ix, &target) in targets.iter().enumerate() {
        let edit =
            edit_cell(project, table, target, col, input).map_err(|err| with_row(err, ix + 1))?;
        edits.push(edit);
    }
    Ok(finish_batch(edits, project))
}

fn unknown_rating(col: &str, value: &str) -> GridError {
    GridError::UnknownRating {
        row: 1,
        col: col.to_owned(),
        value: trunc64(value),
    }
}

pub(crate) fn with_row(err: GridError, row: usize) -> GridError {
    match err {
        GridError::EmptyName { .. } => GridError::EmptyName { row },
        GridError::UnknownRating { col, value, .. } => GridError::UnknownRating { row, col, value },
        GridError::UnknownLink { col, value, .. } => GridError::UnknownLink { row, col, value },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{trunc64, unknown_rating, with_row};
    use crate::grid::GridError;

    #[test]
    fn trunc64_and_with_row_behave_accurately() {
        assert_eq!(trunc64("short"), "short");
        assert_eq!(trunc64(&"a".repeat(80)).chars().count(), 64);
        assert_eq!(trunc64(&"あ".repeat(70)).chars().count(), 64);

        assert_eq!(
            with_row(GridError::EmptyName { row: 1 }, 2),
            GridError::EmptyName { row: 2 }
        );
        assert!(matches!(
            with_row(unknown_rating("impact", "9"), 3),
            GridError::UnknownRating { row: 3, .. }
        ));
        let link = GridError::UnknownLink {
            row: 1,
            col: "component".into(),
            value: "X".into(),
        };
        assert!(matches!(
            with_row(link, 4),
            GridError::UnknownLink { row: 4, .. }
        ));
        assert_eq!(with_row(GridError::NotEditable, 2), GridError::NotEditable);
    }
}
