//! Spreadsheet-style table editing: selection math, row creation and removal,
//! and grid edit errors. No UI toolkit.

use std::collections::HashSet;
use std::ops::RangeInclusive;

use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, ElementId, GoalId, Project, ThreatId,
};

use crate::commands::Edit;
use crate::explorer::Owner;
use crate::view::{Selection, TableId};

mod edit;

pub use edit::{CellEditorKind, CellInput, cell_editor, edit_cell, edit_cells};

/// Why a grid editing, fill, or clipboard paste operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GridError {
    /// Attempted to edit a read-only column (`Id` or `Computed`).
    #[error("column is read-only")]
    ReadOnlyColumn,
    /// Attempted to edit a table that does not accept edits (such as Problems).
    #[error("table is not editable")]
    NotEditable,
    /// Adding a threat or risk row requires at least one asset in scope.
    #[error("create an asset before adding a threat scenario")]
    NoAssetForThreat,
    /// Fill down requires at least two selected rows with a non-empty source.
    #[error("nothing to fill")]
    NothingToFill,
    /// Clipboard TSV input contained no data cells.
    #[error("clipboard paste is empty")]
    EmptyPaste,
    /// Clipboard TSV input exceeded the maximum byte limit.
    #[error("clipboard paste is too large")]
    PasteTooLarge,
    /// Clipboard TSV input exceeded the maximum row count.
    #[error("clipboard paste has too many rows")]
    TooManyRows,
    /// Clipboard TSV input exceeded the maximum column count.
    #[error("clipboard paste has too many columns")]
    TooManyCols,
    /// Clipboard TSV input had an unclosed or invalid quoted field.
    #[error("malformed quote on line {line}")]
    MalformedQuote {
        /// 1-based line number where quoting failed.
        line: usize,
    },
    /// A row name was empty or contained only whitespace.
    #[error("row {row} has an empty name")]
    EmptyName {
        /// 1-based row number with the empty name.
        row: usize,
    },
    /// A rating cell value did not match any option in the methodology.
    #[error("row {row}, column {col}: unknown rating '{value}'")]
    UnknownRating {
        /// 1-based row number.
        row: usize,
        /// Column key or heading.
        col: String,
        /// Rejected input value.
        value: String,
    },
    /// A link cell value did not resolve to any element in the project.
    #[error("row {row}, column {col}: unknown link '{value}'")]
    UnknownLink {
        /// 1-based row number.
        row: usize,
        /// Column key or heading.
        col: String,
        /// Rejected input value.
        value: String,
    },
}

/// Rectangular cell selection in a table grid, defined by an anchor corner
/// and an active cursor corner (inclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSelection {
    /// Row index where the selection started.
    pub anchor_row: usize,
    /// Column index where the selection started.
    pub anchor_col: usize,
    /// Row index of the active cursor cell.
    pub active_row: usize,
    /// Column index of the active cursor cell.
    pub active_col: usize,
}

impl GridSelection {
    /// Creates a single-cell selection at `(row, col)`. Time O(1).
    #[must_use]
    pub const fn single(row: usize, col: usize) -> Self {
        Self {
            anchor_row: row,
            anchor_col: col,
            active_row: row,
            active_col: col,
        }
    }

    /// Moves the active corner to `(row, col)`, keeping the anchor fixed. Time O(1).
    pub fn extend_to(&mut self, row: usize, col: usize) {
        self.active_row = row;
        self.active_col = col;
    }

    /// Moves the active cursor by `(dr, dc)`, saturating within `0..rows` and
    /// `0..cols`. When `extend` is false, the anchor follows the cursor; when
    /// true, the anchor stays put (clamped to bounds). No-op when
    /// `rows == 0 || cols == 0`. Time O(1).
    pub fn move_by(&mut self, dr: isize, dc: isize, rows: usize, cols: usize, extend: bool) {
        if rows == 0 || cols == 0 {
            return;
        }
        let max_row = rows - 1;
        let max_col = cols - 1;
        let next_row = step_clamped(self.active_row, dr, max_row);
        let next_col = step_clamped(self.active_col, dc, max_col);
        self.active_row = next_row;
        self.active_col = next_col;
        if extend {
            self.anchor_row = self.anchor_row.min(max_row);
            self.anchor_col = self.anchor_col.min(max_col);
        } else {
            self.anchor_row = next_row;
            self.anchor_col = next_col;
        }
    }

    /// Clamps anchor and active coordinates into `0..rows` and `0..cols`
    /// (collapsing a zero-sized axis to `0`). Time O(1).
    pub fn clamp(&mut self, rows: usize, cols: usize) {
        let max_row = rows.saturating_sub(1);
        let max_col = cols.saturating_sub(1);
        self.anchor_row = self.anchor_row.min(max_row);
        self.active_row = self.active_row.min(max_row);
        self.anchor_col = self.anchor_col.min(max_col);
        self.active_col = self.active_col.min(max_col);
    }

    /// Inclusive range of selected row indices. Time O(1).
    #[must_use]
    pub fn row_range(&self) -> RangeInclusive<usize> {
        self.anchor_row.min(self.active_row)..=self.anchor_row.max(self.active_row)
    }

    /// Inclusive range of selected column indices. Time O(1).
    #[must_use]
    pub fn col_range(&self) -> RangeInclusive<usize> {
        self.anchor_col.min(self.active_col)..=self.anchor_col.max(self.active_col)
    }

    /// Whether `(row, col)` lies inside the selected rectangle. Time O(1).
    #[must_use]
    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.row_range().contains(&row) && self.col_range().contains(&col)
    }

    /// Number of rows covered by this selection (at least 1). Time O(1).
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.active_row.abs_diff(self.anchor_row).saturating_add(1)
    }
}

fn step_clamped(pos: usize, delta: isize, max: usize) -> usize {
    let clamped = pos.min(max);
    if delta >= 0 {
        clamped.saturating_add(delta.unsigned_abs()).min(max)
    } else {
        clamped.saturating_sub(delta.unsigned_abs())
    }
}

/// Returns the first component belonging to system `sys`, in project order.
/// Time O(components).
pub(crate) fn system_component(project: &Project, sys: ElementId) -> Option<ComponentId> {
    project
        .components()
        .find(|(idx, _, _)| project.component_system(*idx) == Some(sys))
        .map(|(_, id, _)| id)
}

/// Returns the first asset belonging to `owner`, or the first project asset
/// for [`Owner::Project`]. Time O(project elements).
pub(crate) fn default_asset_for_owner(project: &Project, owner: Owner) -> Option<AssetId> {
    match owner {
        Owner::Project => project.assets().next().map(|(_, id, _)| id),
        Owner::System(sys) => {
            let usage = project.system_use();
            let target = usage.set_of([sys]);
            project
                .assets()
                .find(|(idx, _, _)| usage.asset(*idx).intersects(target))
                .map(|(_, id, _)| id)
        }
        Owner::Unassigned => {
            let usage = project.system_use();
            project
                .assets()
                .find(|(idx, _, _)| usage.asset(*idx).is_empty())
                .map(|(_, id, _)| id)
        }
    }
}

/// Returns the first threat category compatible with `asset`'s security
/// properties (or `0` if none matches). Time O(threat categories x properties).
pub(crate) fn default_threat_category(project: &Project, asset: AssetId) -> usize {
    let m = project.methodology();
    let Some(props) = project
        .asset_idx(&asset)
        .and_then(|i| project.asset(i))
        .map(aegis_model::Asset::properties)
    else {
        return 0;
    };
    m.threat_categories()
        .enumerate()
        .find(|(_, cat)| m.violates(*cat).iter().all(|p| props.contains(p)))
        .map_or(0, |(idx, _)| idx)
}

/// Builds an [`Edit`] that adds a new row to `table` under `owner`, returning
/// the edit and the newly created element's [`Selection`].
///
/// Time: O(project elements).
///
/// # Errors
///
/// - [`GridError::NotEditable`] if `table` is [`TableId::Problems`].
/// - [`GridError::EmptyName`] (`row: 1`) if `name` is empty after trimming.
/// - [`GridError::NoAssetForThreat`] if `table` is [`TableId::Threats`] or
///   [`TableId::Risks`] and no asset exists for `owner`.
pub fn add_row_edit(
    project: &Project,
    table: TableId,
    owner: Owner,
    name: impl Into<String>,
) -> Result<(Edit, Selection), GridError> {
    if table == TableId::Problems {
        return Err(GridError::NotEditable);
    }
    let trimmed = name.into();
    let name = trimmed.trim().to_owned();
    if name.is_empty() {
        return Err(GridError::EmptyName { row: 1 });
    }
    match table {
        TableId::Assets => {
            let id = AssetId::new_v7();
            let edit = match owner {
                Owner::System(sys) => match system_component(project, sys) {
                    Some(component) => Edit::AddAssetBound {
                        id,
                        name,
                        properties: vec![0],
                        component,
                    },
                    None => Edit::AddAsset {
                        id,
                        name,
                        properties: vec![0],
                    },
                },
                Owner::Project | Owner::Unassigned => Edit::AddAsset {
                    id,
                    name,
                    properties: vec![0],
                },
            };
            Ok((edit, Selection::Asset(id)))
        }
        TableId::Damage => {
            let id = DamageId::new_v7();
            Ok((Edit::AddDamage { id, name }, Selection::Damage(id)))
        }
        TableId::Threats | TableId::Risks => {
            let asset =
                default_asset_for_owner(project, owner).ok_or(GridError::NoAssetForThreat)?;
            let category = default_threat_category(project, asset);
            let id = ThreatId::new_v7();
            Ok((
                Edit::AddThreat {
                    id,
                    name,
                    asset,
                    category,
                },
                Selection::Threat(id),
            ))
        }
        TableId::Controls => {
            let id = ControlId::new_v7();
            Ok((Edit::AddControl { id, name }, Selection::Control(id)))
        }
        TableId::Goals => {
            let id = GoalId::new_v7();
            Ok((Edit::AddGoal { id, name }, Selection::Goal(id)))
        }
        TableId::Claims => {
            let id = ClaimId::new_v7();
            Ok((Edit::AddClaim { id, name }, Selection::Claim(id)))
        }
        TableId::Problems => Err(GridError::NotEditable),
    }
}

/// Builds an [`Edit`] that deletes the table rows identified by `targets` in
/// one undo step, deduplicating targets while preserving order.
///
/// Returns `None` if `targets` contains no removable table row selections,
/// a single [`Edit`] for one target, or [`Edit::Batch`] for two or more.
/// Time: O(targets).
#[must_use]
pub fn delete_rows_edit(targets: &[Selection]) -> Option<Edit> {
    let mut seen = HashSet::with_capacity(targets.len());
    let mut edits = Vec::with_capacity(targets.len());
    for &target in targets {
        if !seen.insert(target) {
            continue;
        }
        let edit = match target {
            Selection::Asset(asset) => Edit::RemoveAsset { asset },
            Selection::Damage(damage) => Edit::RemoveDamage { damage },
            Selection::Threat(threat) => Edit::RemoveThreat { threat },
            Selection::Control(control) => Edit::RemoveControl { control },
            Selection::Goal(goal) => Edit::RemoveGoal { goal },
            Selection::Claim(claim) => Edit::RemoveClaim { claim },
            _ => continue,
        };
        edits.push(edit);
    }
    match edits.len() {
        0 => None,
        1 => edits.pop(),
        _ => Some(Edit::Batch(edits)),
    }
}
