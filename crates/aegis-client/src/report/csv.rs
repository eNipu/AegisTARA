//! The risk register as CSV and the attack tree as SVG (jobs 041 and 050).
//! These two exports predate the shared document model; the CSV stays
//! English because it is a data export for a spreadsheet, and the formula
//! guard in [`super::csv_field`] is what matters for safety (OWASP CSV
//! injection).

use std::fmt::Write as _;

use aegis_canvas::{RenderOptions, SvgOptions, scene_to_svg};
use aegis_model::TreeId;

use crate::pages;
use crate::workspace::Workspace;

use super::doc::risk_text;
use super::{ReportContext, csv_field};

/// The risk register as CSV. Time O(threats log threats).
#[must_use]
pub fn risk_register_csv(ws: &Workspace, ctx: &ReportContext) -> String {
    let names = ws.project().methodology().element_names();
    let mut out = String::new();
    let _ = writeln!(out, "# {}", csv_field(&ctx.watermark()));
    let _ = write!(
        out,
        "{},{},Deciding {},Impact,{},Risk before controls,Risk after controls,Treatment,Goal or claim,ID\r\n",
        names.threat_scenario.singular,
        names.asset.singular,
        names.damage_scenario.singular.to_lowercase(),
        names.feasibility,
    );
    for r in pages::risk_rows(ws) {
        let fields = [
            r.threat.as_str(),
            &r.asset,
            &r.damage,
            &r.impact,
            &r.feasibility,
            &risk_text(r.inherent.as_ref()),
            &risk_text(r.residual.as_ref()),
            &r.treatment,
            &r.backing,
            &r.visual_id,
        ];
        let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    }
    out
}

/// The SVG of one tree. `None` if the tree does not exist.
#[must_use]
pub fn tree_svg(ws: &Workspace, tree: TreeId, residual: bool) -> Option<String> {
    let scene = crate::view::tree_scene(ws, tree, residual)?;
    Some(scene_to_svg(
        &scene,
        &SvgOptions::light(),
        &RenderOptions::default(),
    ))
}
