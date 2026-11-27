//! Spreadsheet fill-down editing and relation link synchronization.

use std::collections::HashSet;

use aegis_model::Project;

use crate::commands::{Edit, TREATMENTS};
use crate::grid::edit::{
    CellInput, element_name, finish_batch, target_exists, target_matches_table,
};
use crate::grid::{GridError, GridSelection, edit_cell};
use crate::relations::{self, RelationKey};
use crate::table::TableRow;
use crate::view::{ColumnKind, ColumnSpec, Selection, TableId};

/// Computes the minimal link/unlink edits required to make `target`'s targets
/// for `rel_key` match `desired` (deduplicated, preserving order).
///
/// Returns `None` if `rel_key` is unknown or read-only (`rel.link.is_none()`).
/// For 1:1 relations (`!rel.many`), setting a new target emits `(rel.link)`
/// directly without a preceding unlink, and clearing emits `(rel.unlink)` only
/// when currently linked. For N:M relations (`rel.many`), unlinks removed
/// targets and links missing targets.
///
/// Time: O(desired.len() + current_links).
pub(crate) fn sync_link_edits(
    project: &Project,
    rel_key: RelationKey,
    target: Selection,
    desired: &[Selection],
) -> Option<Vec<Edit>> {
    let rel = relations::relation(rel_key)?;
    let link = rel.link?;
    let mut seen = HashSet::with_capacity(desired.len());
    let unique: Vec<Selection> = desired
        .iter()
        .copied()
        .filter(|s| seen.insert(*s))
        .collect();
    let cur = relations::related(project, rel_key, target);
    let mut edits = Vec::new();
    if !rel.many {
        if let Some(&want) = unique.first() {
            if cur.as_slice() != [want]
                && let Some(edit) = link(target, want)
            {
                edits.push(edit);
            }
        } else if let Some(unlink) = rel.unlink {
            for c in cur {
                if let Some(edit) = unlink(target, c) {
                    edits.push(edit);
                }
            }
        }
    } else {
        if let Some(unlink) = rel.unlink {
            for &c in &cur {
                if !seen.contains(&c)
                    && let Some(edit) = unlink(target, c)
                {
                    edits.push(edit);
                }
            }
        }
        let cur_set: HashSet<Selection> = cur.into_iter().collect();
        for want in unique {
            if !cur_set.contains(&want)
                && let Some(edit) = link(target, want)
            {
                edits.push(edit);
            }
        }
    }
    Some(edits)
}

/// Copies editable cell values from the top selected row into all lower
/// selected rows across `visible_cols` in a single undoable [`Edit`].
///
/// Covers `Name`, `Rating` (`properties`, `impact`, `scales`, `treatment`),
/// and editable `Links` columns; skips `Id`, `Computed`, `Text`, read-only
/// `Links` columns, and destination rows with `target: None`.
///
/// Time: O(selected_rows * selected_cols).
///
/// # Errors
///
/// - [`GridError::NotEditable`] if `table` is [`TableId::Problems`] or a
///   destination target is mismatched or missing in `project`.
/// - [`GridError::NothingToFill`] if `rows` or `visible_cols` is empty, fewer
///   than two rows are in range, the selection starts out of bounds, the top
///   row has no valid target, no editable columns are selected, or all lower
///   rows already match the top row.
/// - Any [`GridError`] returned by [`edit_cell`].
pub fn fill_down(
    project: &Project,
    table: TableId,
    rows: &[TableRow],
    visible_cols: &[&ColumnSpec],
    sel: GridSelection,
) -> Result<Edit, GridError> {
    if table == TableId::Problems {
        return Err(GridError::NotEditable);
    }
    if rows.is_empty() || visible_cols.is_empty() || sel.row_count() < 2 {
        return Err(GridError::NothingToFill);
    }
    let src_row = *sel.row_range().start();
    if src_row >= rows.len() {
        return Err(GridError::NothingToFill);
    }
    let end_row = (*sel.row_range().end()).min(rows.len() - 1);
    if end_row <= src_row {
        return Err(GridError::NothingToFill);
    }
    let col_start = *sel.col_range().start();
    if col_start >= visible_cols.len() {
        return Err(GridError::NothingToFill);
    }
    let col_end = (*sel.col_range().end()).min(visible_cols.len() - 1);
    let Some(src_target) = rows[src_row].target else {
        return Err(GridError::NothingToFill);
    };
    if !target_matches_table(project, table, src_target) || !target_exists(project, src_target) {
        return Err(GridError::NothingToFill);
    }

    let dst_rows = &rows[(src_row + 1)..=end_row];
    let mut edits = Vec::new();
    for &col in &visible_cols[col_start..=col_end] {
        match col.kind {
            ColumnKind::Id | ColumnKind::Computed | ColumnKind::Text => {}
            ColumnKind::Name => {
                let Some(src_name) = element_name(project, src_target) else {
                    continue;
                };
                let input = CellInput::Text(src_name.clone());
                for row in dst_rows {
                    let Some(dst) = row.target else { continue };
                    if !target_matches_table(project, table, dst) || !target_exists(project, dst) {
                        return Err(GridError::NotEditable);
                    }
                    if element_name(project, dst).as_deref() == Some(src_name.trim()) {
                        continue;
                    }
                    edits.push(edit_cell(project, table, dst, col, &input)?);
                }
            }
            ColumnKind::Rating => {
                let Some(input) = source_rating_input(project, table, src_target, col.key) else {
                    continue;
                };
                for row in dst_rows {
                    let Some(dst) = row.target else { continue };
                    edits.push(edit_cell(project, table, dst, col, &input)?);
                }
            }
            ColumnKind::Links(rel_key) => {
                if relations::relation(rel_key).and_then(|r| r.link).is_none() {
                    continue;
                }
                let desired = relations::related(project, rel_key, src_target);
                for row in dst_rows {
                    let Some(dst) = row.target else { continue };
                    if !target_matches_table(project, table, dst) || !target_exists(project, dst) {
                        return Err(GridError::NotEditable);
                    }
                    if let Some(link_edits) = sync_link_edits(project, rel_key, dst, &desired) {
                        edits.extend(link_edits);
                    }
                }
            }
        }
    }

    if edits.is_empty() {
        return Err(GridError::NothingToFill);
    }
    let batch = finish_batch(edits, project);
    if matches!(&batch, Edit::Batch(v) if v.is_empty()) {
        return Err(GridError::NothingToFill);
    }
    Ok(batch)
}

fn source_rating_input(
    project: &Project,
    table: TableId,
    src_target: Selection,
    col_key: &str,
) -> Option<CellInput> {
    match (table, col_key, src_target) {
        (TableId::Assets, "properties", Selection::Asset(id)) => {
            let asset = project.asset_idx(&id).and_then(|i| project.asset(i))?;
            let props = asset.properties();
            let flags = project
                .methodology()
                .properties()
                .map(|p| props.contains(p))
                .collect();
            Some(CellInput::MultiChoice(flags))
        }
        (TableId::Damage, "impact", Selection::Damage(id)) => {
            let damage = project
                .damage_scenario_idx(&id)
                .and_then(|i| project.damage_scenario(i))?;
            let m = project.methodology();
            let levels = m
                .impact_categories()
                .map(|cat| {
                    let level = damage.impact(cat)?;
                    m.levels(cat).position(|l| l == level)
                })
                .collect();
            Some(CellInput::Impacts(levels))
        }
        (TableId::Threats, "scales", Selection::Threat(id)) => {
            let t = project
                .threat_scenario_idx(&id)
                .and_then(|i| project.threat_scenario(i))?;
            Some(CellInput::SingleChoice(
                t.scales().map(|yes| usize::from(!yes)),
            ))
        }
        (TableId::Risks, "treatment", Selection::Threat(id)) => {
            let t = project
                .threat_scenario_idx(&id)
                .and_then(|i| project.threat_scenario(i))?;
            let choice = t
                .treatment()
                .and_then(|tr| TREATMENTS.iter().position(|x| *x == tr));
            Some(CellInput::SingleChoice(choice))
        }
        (TableId::SharedSteps, "feasibility", Selection::SharedStep(id)) => {
            let step = project.shared_step(project.shared_step_idx(&id)?)?;
            let selected = step.feasibility().and_then(|level| {
                project
                    .methodology()
                    .feasibility_levels()
                    .position(|(l, _, _)| l == level)
            });
            Some(CellInput::SingleChoice(selected))
        }
        (TableId::SharedSteps, "attack-type", Selection::SharedStep(id)) => {
            let step = project.shared_step(project.shared_step_idx(&id)?)?;
            let selected = step.attack_type().and_then(|attack| {
                project
                    .methodology()
                    .attack_types()
                    .position(|a| a == attack)
            });
            Some(CellInput::SingleChoice(selected))
        }
        _ => None,
    }
}
