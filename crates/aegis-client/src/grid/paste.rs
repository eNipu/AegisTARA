//! Spreadsheet TSV paste engine: updates existing rows and appends new rows
//! across all editable tables in one undoable [`Edit`].

use std::collections::HashSet;

use aegis_model::{
    AssetId, ClaimId, ControlId, DamageId, GoalId, ItemRef, PartyId, Project, SharedStepId,
    ThreatId,
};

use crate::commands::{Edit, TREATMENTS};
use crate::explorer::Owner;
use crate::grid::edit::{
    CellInput, element_name, finish_batch, target_exists, target_matches_table, trunc64, with_row,
};
use crate::grid::fill::sync_link_edits;
use crate::grid::{
    GridError, default_asset_for_owner, default_threat_category, edit_cell, parse_tsv,
    system_component,
};
use crate::i18n::Catalog;
use crate::pages::{Vocabulary, selection_visual_id};
use crate::relations::{self, RelationKey};
use crate::table::{TableRow, selection_label};
use crate::view::{ColumnKind, ColumnSpec, Selection, TableId};

const CANONICAL_TREATMENTS: [&str; 4] = ["avoid", "reduce", "share", "retain"];

struct PasteCtx<'a> {
    project: &'a Project,
    table: TableId,
    owner: Owner,
    visible_cols: &'a [&'a ColumnSpec],
    start_col: usize,
    vocab: &'a Vocabulary,
    catalog: &'a Catalog,
    edits: Vec<Edit>,
}

/// Parses clipboard TSV text and applies it to `table` starting at
/// `(start_row, start_col)` across `visible_cols`, updating existing rows and
/// appending new rows past `rows.len()` in a single undoable [`Edit`].
///
/// Validates all pasted rows before returning an [`Edit`], so malformed ratings
/// or unknown links fail with 1-based row/column errors without mutating the
/// project. Excess trailing columns beyond `visible_cols` are ignored silently,
/// as are `Id`, `Computed`, `Text`, and read-only `Links` columns on existing
/// rows. Pasting identical values onto existing rows succeeds with
/// `Edit::Batch(vec![])` after [`Edit::changing`].
///
/// Time: O(pasted_rows * (cols + candidates)).
///
/// # Errors
///
/// - [`GridError::NotEditable`] if `table` is [`TableId::Problems`] or an
///   existing row target is mismatched or missing in `project`.
/// - [`GridError::EmptyPaste`] if `tsv` is empty or touches no editable cells
///   and appends no non-blank rows.
/// - [`GridError::PasteTooLarge`], [`GridError::TooManyRows`],
///   [`GridError::TooManyCols`], or [`GridError::MalformedQuote`] from
///   [`parse_tsv`].
/// - [`GridError::EmptyName`] (1-based `row`) if an edited or appended row has
///   an empty or missing `"name"` cell.
/// - [`GridError::UnknownRating`] (1-based `row`, `col`, value truncated to 64
///   chars) if a rating token does not match the methodology or vocabulary.
/// - [`GridError::UnknownLink`] (1-based `row`, `col`, value truncated to 64
///   chars) if a link token does not resolve to any candidate or supplies
///   multiple targets for a 1:1 relation.
/// - [`GridError::NoAssetForThreat`] if an appended threat/risk row omits
///   `"asset"` and `owner` has no asset in scope.
#[allow(clippy::too_many_arguments)]
pub fn paste_tsv(
    project: &Project,
    table: TableId,
    owner: Owner,
    rows: &[TableRow],
    visible_cols: &[&ColumnSpec],
    start_row: usize,
    start_col: usize,
    tsv: &str,
    vocab: &Vocabulary,
    catalog: &Catalog,
) -> Result<Edit, GridError> {
    if table == TableId::Problems {
        return Err(GridError::NotEditable);
    }
    let grid = parse_tsv(tsv)?;
    let mut ctx = PasteCtx {
        project,
        table,
        owner,
        visible_cols,
        start_col,
        vocab,
        catalog,
        edits: Vec::new(),
    };

    let mut touched = false;
    for (dr, pasted_row) in grid.iter().enumerate() {
        let row = dr + 1;
        let dest = start_row.saturating_add(dr);
        touched |= if dest < rows.len() {
            ctx.existing_row(rows[dest].target, pasted_row, row)?
        } else {
            ctx.new_row(pasted_row, row)?
        };
    }

    if !touched && ctx.edits.is_empty() {
        return Err(GridError::EmptyPaste);
    }
    Ok(finish_batch(ctx.edits, project))
}

impl PasteCtx<'_> {
    fn check_target(&self, target: Selection) -> Result<(), GridError> {
        if !target_matches_table(self.table, target) || !target_exists(self.project, target) {
            return Err(GridError::NotEditable);
        }
        Ok(())
    }

    fn existing_row(
        &mut self,
        target: Option<Selection>,
        pasted: &[String],
        row: usize,
    ) -> Result<bool, GridError> {
        let Some(target) = target else {
            return Ok(false);
        };
        let mut touched = false;
        for (dc, raw) in pasted.iter().enumerate() {
            let Some(&col) = self.visible_cols.get(self.start_col.saturating_add(dc)) else {
                continue;
            };
            match col.kind {
                ColumnKind::Id | ColumnKind::Computed | ColumnKind::Text => {}
                ColumnKind::Name => {
                    self.check_target(target)?;
                    let trimmed = raw.trim();
                    if trimmed.is_empty() {
                        return Err(GridError::EmptyName { row });
                    }
                    touched = true;
                    if element_name(self.project, target).as_deref() != Some(trimmed) {
                        let input = CellInput::Text(trimmed.to_owned());
                        let edit = edit_cell(self.project, self.table, target, col, &input)
                            .map_err(|e| with_row(e, row))?;
                        self.edits.push(edit);
                    }
                }
                ColumnKind::Rating => {
                    self.check_target(target)?;
                    let input = self.parse_rating(col.key, raw, row)?;
                    let edit = edit_cell(self.project, self.table, target, col, &input)
                        .map_err(|e| with_row(e, row))?;
                    self.edits.push(edit);
                    touched = true;
                }
                ColumnKind::Links(rel_key) => {
                    if relations::relation(rel_key).and_then(|r| r.link).is_none() {
                        continue;
                    }
                    self.check_target(target)?;
                    let desired = self.parse_links(rel_key, raw, row, col.key)?;
                    if let Some(link_edits) =
                        sync_link_edits(self.project, rel_key, target, &desired)
                    {
                        self.edits.extend(link_edits);
                        touched = true;
                    }
                }
            }
        }
        Ok(touched)
    }

    fn new_row(&mut self, pasted: &[String], row: usize) -> Result<bool, GridError> {
        if pasted.iter().all(|cell| cell.trim().is_empty()) {
            return Ok(false);
        }
        let cells: Vec<(&str, &str)> = pasted
            .iter()
            .enumerate()
            .filter_map(|(dc, raw)| {
                self.visible_cols
                    .get(self.start_col.saturating_add(dc))
                    .map(|col| (col.key, raw.as_str()))
            })
            .collect();
        let name = cell_val(&cells, "name")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or(GridError::EmptyName { row })?
            .to_owned();

        match self.table {
            TableId::Assets => self.append_asset(&cells, name, row)?,
            TableId::Damage => self.append_damage(&cells, name, row)?,
            TableId::Threats | TableId::Risks => self.append_threat(&cells, name, row)?,
            TableId::Controls => self.edits.push(Edit::AddControl {
                id: ControlId::new_v7(),
                name,
            }),
            TableId::Goals => {
                let parties = self.parse_parties(RelationKey::GOAL_RESPONSIBLE, &cells, row)?;
                let id = GoalId::new_v7();
                self.edits.push(Edit::AddGoal { id, name });
                self.edits
                    .extend(parties.into_iter().map(|party| Edit::LinkGoalParty {
                        goal: id,
                        party,
                        linked: true,
                    }));
            }
            TableId::Claims => {
                let parties = self.parse_parties(RelationKey::CLAIM_RESPONSIBLE, &cells, row)?;
                let id = ClaimId::new_v7();
                self.edits.push(Edit::AddClaim { id, name });
                self.edits
                    .extend(parties.into_iter().map(|party| Edit::LinkClaimParty {
                        claim: id,
                        party,
                        linked: true,
                    }));
            }
            TableId::SharedSteps => self.append_shared_step(&cells, name, row)?,
            TableId::Problems => return Err(GridError::NotEditable),
        }
        Ok(true)
    }

    fn append_asset(
        &mut self,
        cells: &[(&str, &str)],
        name: String,
        row: usize,
    ) -> Result<(), GridError> {
        let properties = if let Some(raw) = cell_val(cells, "properties") {
            self.parse_properties(raw, row, "properties")?
                .into_iter()
                .enumerate()
                .filter_map(|(i, on)| on.then_some(i))
                .collect()
        } else {
            vec![0]
        };
        let component =
            match self.single_link(RelationKey::ASSET_COMPONENT, "component", cells, row)? {
                Some(Selection::Component(cid)) => Some(cid),
                Some(_) => return Err(unknown_link(row, "component", "")),
                None => match self.owner {
                    Owner::System(sys) => system_component(self.project, sys),
                    Owner::Project | Owner::Unassigned => None,
                },
            };
        let id = AssetId::new_v7();
        self.edits.push(match component {
            Some(component) => Edit::AddAssetBound {
                id,
                name,
                properties,
                component,
            },
            None => Edit::AddAsset {
                id,
                name,
                properties,
            },
        });
        Ok(())
    }

    fn append_damage(
        &mut self,
        cells: &[(&str, &str)],
        name: String,
        row: usize,
    ) -> Result<(), GridError> {
        let levels = cell_val(cells, "impact")
            .map(|raw| self.parse_impacts(raw, row, "impact"))
            .transpose()?;
        let id = DamageId::new_v7();
        self.edits.push(Edit::AddDamage { id, name });
        if let Some(levels) = levels {
            for (category, level) in levels.into_iter().enumerate() {
                if let Some(lvl) = level {
                    self.edits.push(Edit::SetImpact {
                        damage: id,
                        category,
                        level: Some(lvl),
                    });
                }
            }
        }
        Ok(())
    }

    fn append_threat(
        &mut self,
        cells: &[(&str, &str)],
        name: String,
        row: usize,
    ) -> Result<(), GridError> {
        let asset = match self.single_link(RelationKey::THREAT_ASSET, "asset", cells, row)? {
            Some(Selection::Asset(aid)) => aid,
            Some(_) => return Err(unknown_link(row, "asset", "")),
            None => default_asset_for_owner(self.project, self.owner)
                .ok_or(GridError::NoAssetForThreat)?,
        };
        let scales = cell_val(cells, "scales")
            .map(|raw| self.parse_scales(raw, row, "scales"))
            .transpose()?
            .flatten();
        let treatment = cell_val(cells, "treatment")
            .map(|raw| self.parse_treatment(raw, row, "treatment"))
            .transpose()?
            .flatten();

        let id = ThreatId::new_v7();
        let category = default_threat_category(self.project, asset);
        self.edits.push(Edit::AddThreat {
            id,
            name,
            asset,
            category,
        });
        if let Some(scales) = scales {
            self.edits.push(Edit::SetThreatScaling {
                threat: id,
                scales: Some(scales),
                reason: String::new(),
            });
        }
        if let Some(treatment) = treatment {
            self.edits.push(Edit::SetTreatment {
                threat: id,
                treatment: Some(treatment),
            });
        }
        Ok(())
    }

    fn append_shared_step(
        &mut self,
        cells: &[(&str, &str)],
        name: String,
        row: usize,
    ) -> Result<(), GridError> {
        let id = SharedStepId::new_v7();
        self.edits.push(Edit::AddSharedStep { id, name });
        if let Some(ix) = cell_val(cells, "feasibility")
            .map(|raw| self.parse_feasibility(raw, row, "feasibility"))
            .transpose()?
            .flatten()
            && let Some((level, _, _)) = self.project.methodology().feasibility_levels().nth(ix)
        {
            self.edits.push(Edit::SetSharedStepFeasibility {
                shared_step: id,
                level: Some(level),
            });
        }
        if let Some(ix) = cell_val(cells, "attack-type")
            .map(|raw| self.parse_attack_type(raw, row, "attack-type"))
            .transpose()?
            .flatten()
            && let Some(attack_type) = self.project.methodology().attack_types().nth(ix)
        {
            self.edits.push(Edit::SetSharedStepAttackType {
                shared_step: id,
                attack_type: Some(attack_type),
            });
        }
        let via = match self.single_link(RelationKey::SHARED_STEP_VIA, "via", cells, row)? {
            Some(Selection::Component(id)) => Some(ItemRef::Component(id)),
            Some(Selection::Interface(id)) => Some(ItemRef::Interface(id)),
            Some(_) => return Err(unknown_link(row, "via", "")),
            None => None,
        };
        if let Some(via) = via {
            self.edits.push(Edit::SetSharedStepVia {
                shared_step: id,
                via: Some(via),
            });
        }
        Ok(())
    }

    fn single_link(
        &self,
        rel_key: RelationKey,
        col: &str,
        cells: &[(&str, &str)],
        row: usize,
    ) -> Result<Option<Selection>, GridError> {
        let Some(raw) = cell_val(cells, col).filter(|s| !s.trim().is_empty()) else {
            return Ok(None);
        };
        Ok(self.parse_links(rel_key, raw, row, col)?.into_iter().next())
    }

    fn parse_parties(
        &self,
        rel_key: RelationKey,
        cells: &[(&str, &str)],
        row: usize,
    ) -> Result<Vec<PartyId>, GridError> {
        let Some(raw) = cell_val(cells, "responsible") else {
            return Ok(Vec::new());
        };
        Ok(self
            .parse_links(rel_key, raw, row, "responsible")?
            .into_iter()
            .filter_map(|sel| match sel {
                Selection::Party(pid) => Some(pid),
                _ => None,
            })
            .collect())
    }

    fn parse_rating(&self, col: &str, raw: &str, row: usize) -> Result<CellInput, GridError> {
        match (self.table, col) {
            (TableId::Assets, "properties") => Ok(CellInput::MultiChoice(
                self.parse_properties(raw, row, col)?,
            )),
            (TableId::Damage, "impact") => {
                Ok(CellInput::Impacts(self.parse_impacts(raw, row, col)?))
            }
            (TableId::Threats, "scales") => Ok(CellInput::SingleChoice(
                self.parse_scales(raw, row, col)?
                    .map(|yes| usize::from(!yes)),
            )),
            (TableId::Risks, "treatment") => Ok(CellInput::SingleChoice(
                self.parse_treatment(raw, row, col)?,
            )),
            (TableId::SharedSteps, "feasibility") => Ok(CellInput::SingleChoice(
                self.parse_feasibility(raw, row, col)?,
            )),
            (TableId::SharedSteps, "attack-type") => Ok(CellInput::SingleChoice(
                self.parse_attack_type(raw, row, col)?,
            )),
            _ => Err(GridError::ReadOnlyColumn),
        }
    }

    fn parse_properties(&self, raw: &str, row: usize, col: &str) -> Result<Vec<bool>, GridError> {
        let m = self.project.methodology();
        let props: Vec<_> = m.properties().collect();
        let mut flags = vec![false; props.len()];
        for part in raw.split([',', ';']) {
            let tok = part.trim();
            if tok.is_empty() {
                return Err(unknown_rating(row, col, raw));
            }
            let Some(ix) = props.iter().enumerate().position(|(ix, &p)| {
                self.vocab.properties.get(ix).is_some_and(|l| eq_ci(tok, l))
                    || m.property_label(p).is_some_and(|l| eq_ci(tok, l))
            }) else {
                return Err(unknown_rating(row, col, tok));
            };
            flags[ix] = true;
        }
        Ok(flags)
    }

    fn parse_impacts(
        &self,
        raw: &str,
        row: usize,
        col: &str,
    ) -> Result<Vec<Option<usize>>, GridError> {
        let m = self.project.methodology();
        let mut levels = vec![None; m.impact_category_count()];
        if raw.trim().is_empty() {
            return Ok(levels);
        }
        for part in raw.split([',', ';']) {
            let item = part.trim();
            let Some((cat_s, lvl_s)) = item.split_once(':') else {
                return Err(unknown_rating(row, col, non_empty(item, raw)));
            };
            let (cat_s, lvl_s) = (cat_s.trim(), lvl_s.trim());
            let Some((cat_ix, cat)) = m.impact_categories().enumerate().find(|&(ix, cat)| {
                self.vocab
                    .impact_categories
                    .get(ix)
                    .is_some_and(|v| eq_ci(cat_s, &v.label))
                    || m.impact_category_label(cat)
                        .is_some_and(|l| eq_ci(cat_s, l))
            }) else {
                return Err(unknown_rating(row, col, non_empty(cat_s, item)));
            };
            let v_levels = self.vocab.impact_categories.get(cat_ix).map(|v| &v.levels);
            let Some(lvl_ix) = m.levels(cat).enumerate().find_map(|(i, lvl)| {
                let hit = v_levels
                    .and_then(|ls| ls.get(i))
                    .is_some_and(|l| eq_ci(lvl_s, l))
                    || m.level_label(cat, lvl).is_some_and(|l| eq_ci(lvl_s, l));
                hit.then_some(i)
            }) else {
                return Err(unknown_rating(row, col, non_empty(lvl_s, item)));
            };
            levels[cat_ix] = Some(lvl_ix);
        }
        Ok(levels)
    }

    fn parse_scales(&self, raw: &str, row: usize, col: &str) -> Result<Option<bool>, GridError> {
        let tok = raw.trim();
        if tok.is_empty() {
            return Ok(None);
        }
        if eq_ci(tok, "yes") || eq_ci(tok, "true") || eq_ci(tok, &self.catalog.tr("yes")) {
            return Ok(Some(true));
        }
        if eq_ci(tok, "no") || eq_ci(tok, "false") || eq_ci(tok, &self.catalog.tr("no")) {
            return Ok(Some(false));
        }
        Err(unknown_rating(row, col, tok))
    }

    fn parse_treatment(
        &self,
        raw: &str,
        row: usize,
        col: &str,
    ) -> Result<Option<usize>, GridError> {
        let tok = raw.trim();
        if tok.is_empty() {
            return Ok(None);
        }
        self.vocab
            .treatments
            .iter()
            .take(TREATMENTS.len())
            .position(|l| eq_ci(tok, l))
            .or_else(|| CANONICAL_TREATMENTS.iter().position(|&n| eq_ci(tok, n)))
            .map(Some)
            .ok_or_else(|| unknown_rating(row, col, tok))
    }

    fn parse_feasibility(
        &self,
        raw: &str,
        row: usize,
        col: &str,
    ) -> Result<Option<usize>, GridError> {
        let tok = raw.trim();
        if tok.is_empty() {
            return Ok(None);
        }
        let m = self.project.methodology();
        self.vocab
            .feasibility
            .iter()
            .position(|label| eq_ci(tok, label))
            .or_else(|| {
                m.feasibility_levels().position(|(level, _, _)| {
                    m.feasibility_level_label(level)
                        .is_some_and(|label| eq_ci(tok, label))
                })
            })
            .map(Some)
            .ok_or_else(|| unknown_rating(row, col, tok))
    }

    fn parse_attack_type(
        &self,
        raw: &str,
        row: usize,
        col: &str,
    ) -> Result<Option<usize>, GridError> {
        let tok = raw.trim();
        if tok.is_empty() {
            return Ok(None);
        }
        let m = self.project.methodology();
        self.vocab
            .attack_types
            .iter()
            .position(|label| eq_ci(tok, label))
            .or_else(|| {
                m.attack_types().position(|attack| {
                    m.attack_type_label(attack)
                        .is_some_and(|label| eq_ci(tok, label))
                })
            })
            .map(Some)
            .ok_or_else(|| unknown_rating(row, col, tok))
    }

    fn parse_links(
        &self,
        rel_key: RelationKey,
        raw: &str,
        row: usize,
        col: &str,
    ) -> Result<Vec<Selection>, GridError> {
        let Some(rel) = relations::relation(rel_key) else {
            return Err(unknown_link(row, col, raw));
        };
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let tokens: Vec<&str> = raw
            .split([',', ';'])
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() || (!rel.many && tokens.len() > 1) {
            return Err(unknown_link(row, col, raw));
        }
        let cands: Vec<(Selection, String, String)> = relations::candidates(self.project, rel_key)
            .into_iter()
            .map(|c| {
                (
                    c,
                    selection_visual_id(self.project, c),
                    selection_label(self.project, c),
                )
            })
            .collect();
        let mut seen = HashSet::with_capacity(tokens.len());
        let mut out = Vec::with_capacity(tokens.len());
        for tok in tokens {
            let Some(sel) = resolve_candidate(&cands, tok) else {
                return Err(unknown_link(row, col, tok));
            };
            if seen.insert(sel) {
                out.push(sel);
            }
        }
        Ok(out)
    }
}

fn cell_val<'a>(cells: &[(&str, &'a str)], key: &str) -> Option<&'a str> {
    cells.iter().find(|&&(k, _)| k == key).map(|&(_, v)| v)
}

fn non_empty<'a>(a: &'a str, fallback: &'a str) -> &'a str {
    if a.is_empty() { fallback } else { a }
}

fn resolve_candidate(cands: &[(Selection, String, String)], token: &str) -> Option<Selection> {
    cands
        .iter()
        .find(|(_, vis, _)| eq_ci(token, vis))
        .or_else(|| {
            cands
                .iter()
                .find(|(_, vis, name)| eq_ci(token, &format!("{vis} — {name}")))
        })
        .or_else(|| cands.iter().find(|(_, _, name)| eq_ci(token, name)))
        .map(|&(sel, _, _)| sel)
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

fn unknown_rating(row: usize, col: &str, value: &str) -> GridError {
    GridError::UnknownRating {
        row,
        col: col.to_owned(),
        value: trunc64(value),
    }
}

fn unknown_link(row: usize, col: &str, value: &str) -> GridError {
    GridError::UnknownLink {
        row,
        col: col.to_owned(),
        value: trunc64(value),
    }
}
