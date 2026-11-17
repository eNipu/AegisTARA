//! Table cells built from the column specs (UI-14). No UI toolkit.
//!
//! A renderer asks [`TableId::columns`] what a table shows and
//! [`table_rows`] for the cells, so no table has its own column code.

use aegis_canvas::Tone;

use crate::i18n::Catalog;
use crate::pages::{self, Vocabulary};
use crate::view::{RiskView, Selection, TableId, threat_rows};
use crate::workspace::Workspace;

/// One cell: its text and, for a risk level, its color bucket.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    /// What the cell shows.
    pub text: String,
    /// The color bucket of a risk level.
    pub tone: Option<Tone>,
}

/// One row: the element it opens and one cell per column of the table,
/// in the order of [`TableId::columns`], hidden columns included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow {
    /// The element the row stands for; `None` for a problem with no target.
    pub target: Option<Selection>,
    /// One cell per column.
    pub cells: Vec<Cell>,
}

fn text(text: impl Into<String>) -> Option<Cell> {
    Some(Cell {
        text: text.into(),
        tone: None,
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
        },
        |r| Cell {
            text: r.label.clone(),
            tone: Some(r.tone),
        },
    ))
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
        })
        .collect()
}

impl Cell {
    fn from_key(key: &str) -> Cell {
        Cell {
            text: format!("[{key}]"),
            tone: None,
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
    let unrated = || catalog.tr("not-rated");
    match table {
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
    }
}
